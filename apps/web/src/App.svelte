<script lang="ts">
  // Boot (TODO #28): the ENGINE WORKER does everything — pack fetch, home,
  // wasm, open, warm, and later the deferred R&D pack — while this thread
  // paints the splash from its progress messages. Fonts load here too, for
  // PAINTING; the worker loads its own copy for layout measurement.
  //
  // The splash is a SPLASH. A previous build painted last session's chapter
  // here as a live mini-reader; it looked like the app but answered nothing,
  // and reading text you can't touch reads as broken, not fast (feedback
  // 2026-07-26). Honest progress beats a decoy — the work now goes into
  // making the wait short rather than disguising it.
  import { bootErrorCopy } from "./engine/bootError";
  import { EngineRpc, type WorkerProgress } from "./engine/worker-client";
  import { churchFromQuery, hasChurch, sharedAtRef, startsAsNewBeliever } from "./shell/church";
  import { initSession, type Session } from "./state/session.svelte";
  import { dispatchLink } from "./study/links";
  import FirstRun from "./shell/FirstRun.svelte";
  import Shell from "./shell/Shell.svelte";

  // PREPARE, not download. Boot's own first message says the same thing now
  // (engine/boot.ts) — this is the value the splash paints in the milliseconds
  // before the worker has said anything at all, and a warm boot, which downloads
  // nothing, must not open by claiming to be fetching (audit D-11).
  let phase = $state<WorkerProgress>({ phase: "prepare" });
  let error = $state<string | null>(null);
  let session = $state<Session | null>(null);

  /** A refKey ("Gen 1:7", "1John 3:16") → the core's `go:` verb, split on the
   *  LAST space — the same rule as core `go_uri` and `VRef::parse_ref_key`.
   *
   *  Every OSIS id the corpus ships is one word ("1John", "2Chr"), so the old
   *  `replace(" ", ":")` happened to agree for all 66 books; it disagreed with
   *  the contract, which lets a book id hold a space, and a disagreement here is
   *  silent — the verb parses into a book nobody has and the tap does nothing.
   *  The same line sits in MemorizeHost and StudyPanel; all three go when refKey
   *  parse/format reaches the ABI. */
  const goUri = (refKey: string): string => `go:${refKey.replace(/ (?=\S*$)/, ":")}`;

  async function start(): Promise<void> {
    try {
      const rpc = new EngineRpc();
      rpc.onProgress = (p) => (phase = p);
      // Phones defer the machine-tier auto-download (2026-07-26): the shell
      // offers an explicit "load analysis" action instead of spending ~4 MB
      // and worker time behind the reader's back.
      const deferRnd = matchMedia("(max-width: 700px)").matches;
      const [info] = await Promise.all([
        rpc.boot({ deferRnd }),
        document.fonts.load('18px "EB Garamond"'),
        document.fonts.load('italic 18px "EB Garamond"'),
        document.fonts.load('bold 18px "EB Garamond"'),
      ]);
      // The worker measures layout with its OWN FontFaceSet. If its load failed
      // it would silently measure platform-serif metrics while this thread
      // paints real Garamond, and lines would wrap where they are not drawn —
      // so say so in the console rather than let it pass as a rendering quirk.
      if (info.fontFaces !== 2) {
        console.warn(
          `[plumbline] engine worker loaded ${info.fontFaces}/2 reader faces — ` +
            `layout is being measured with fallback metrics`,
        );
      }
      // Prime what synchronous readers need on their first frame: the theme
      // palettes and the TOC/canon shape.
      const [light, dark, night] = await Promise.all([
        rpc.static("themePalette", "light"),
        rpc.static("themePalette", "dark"),
        rpc.static("themePalette", "night"),
      ]);
      const s = initSession(rpc, info, { light, dark, night }, info.bundledOn);
      // A shared link can carry the sender's church. Save it as this reader's
      // own (theirs wins if they've already set one), then strip it from the
      // address bar so a bookmark or a reload isn't a link about a church.
      const shared = churchFromQuery(location.search);
      s.startAsNewBeliever = startsAsNewBeliever(location.search);
      if (shared && !s.config.church?.name) {
        s.config.church = shared;
        s.sharedByChurch = shared;
        s.saveConfig();
      } else if (shared) {
        s.sharedByChurch = shared; // shown in the welcome, not saved over theirs
      }
      // A shared PASSAGE opens where it points (`?at=Ps 23:1`) — the QR on the
      // Present end card hands over the weave, not just the app (2026-07-27).
      const at = sharedAtRef(location.search);
      if (shared || s.startAsNewBeliever || at) {
        history.replaceState(null, "", location.pathname + location.hash);
      }
      // Returning readers never see the welcome, so without this a link's
      // church would be saved with no sign it happened (feedback 2026-07-27).
      if (hasChurch(shared) && !s.showFirstRun) {
        s.showToast(`Home church set to ${shared.name} — tap Church to visit them`);
      }
      // "Deferred" has to mean the reader WANTS the machine tier and its download
      // was held back — not merely that no download happened. Since the tiers
      // became opt-in (2026-07-28) those two came apart: with the tier off,
      // `rndAuto` is correctly false on every device, and without the last clause
      // every phone would be shown StudyPanel's "Load analysis" offer for a tier
      // its reader had never asked for.
      s.rndDeferred = deferRnd && !info.rndAuto && s.config.machineAnalysis === true;
      await Promise.all([s.fetchQ("toc"), s.fetchQ("canonSegments")]);
      // An incoming ADDRESS beats the restored position — that is the whole point
      // of a link somebody sent, and of a bookmark. Applied here for two reasons:
      // after the TOC, so a hash naming a book nobody has falls through to the
      // restored session instead of opening a pane on nothing; and before the
      // shell mounts, so a routed arrival never flashes last session's chapter
      // first. `history: false` — the reader never saw the restored chapter, so
      // it is not somewhere for Back to return to.
      //
      // ARRIVALS ONLY. A reload's address is not incoming information — it is the
      // one this app stamped itself last session — while the config underneath it
      // may have been REPLACED: restoring a backup mutes every config write and
      // reloads, so honouring the address there would open the chapter the old
      // session was in instead of the one the backup was last in
      // (e2e/legacy-restore.spec.ts is the guard: the backup's Revelation 22
      // against the live John 3). On every other reload the two agree anyway,
      // because `pagehide` flushes the session before the document goes.
      const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
      // No entry at all (an engine without the API) is treated as an arrival: a
      // shared link that opens nowhere is the worse of the two failures.
      const routed = nav?.type === "reload" ? null : s.routeFromHash(location.hash);
      if (routed) s.navigate(0, routed.book, routed.chapter, null, { history: false });
      s.installRouter();
      session = s;
      // After the TOC is in, so navigation clamps against a real canon. AFTER the
      // hash too, and deliberately: `?at=` names a verse, so it is the more
      // specific of the two and wins when a link carries both.
      if (at) void dispatchLink(s, goUri(at));
      // The on-device boot numbers (also under Settings → boot diagnostics).
      void rpc.bootTrace().then((t) => {
        s.bootTrace = t;
        console.table(t.map(([stage, ms]) => ({ stage, ms })));
      });
      // Idle work: make this visit enough to run offline next time, sweep the
      // versions we've moved off, and notice a deploy that landed since.
      const idle = globalThis.requestIdleCallback ?? ((f: () => void) => setTimeout(f, 1200));
      idle(() => {
        void s.sweepCaches();
        void s.checkForUpdate();
      });
      // An installed PWA is resumed far more often than it is launched, so
      // coming back to the foreground is the moment worth re-checking (the
      // check throttles itself).
      document.addEventListener("visibilitychange", () => {
        if (document.visibilityState === "visible") void s.checkForUpdate();
      });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  start();

  // The address bar follows pane 0 wherever it goes. An EFFECT rather than a line
  // inside `navigate`, because a pane's chapter moves down half a dozen paths —
  // the canon strip, a history step, a weave tapped in Explore, the passage
  // navigator, `?at=`, session restore — and the URL has to follow all of them,
  // not the ones somebody remembered to instrument.
  $effect(() => {
    session?.syncUrl();
  });

  // The phone's Back button, wired to the surface stack: an open surface owns a
  // history entry, so Back closes it instead of exiting the PWA. Android has had
  // this since it shipped (BackHandler); on the web, Back out of a study sheet
  // left an installed app entirely — there was nothing under it but the launch.
  $effect(() => {
    const s = session;
    if (!s) return;
    if (s.transientOpen) s.pushSurfaceEntry();
    else s.dropSurfaceEntry();
  });

  const phaseLabel = $derived(
    phase.phase === "download"
      ? `Fetching scripture data — ${Math.round((phase.fraction ?? 0) * 100)}%`
      : phase.phase === "prepare"
        ? "Preparing the study engine…"
        : phase.phase === "warm"
          ? "Building the analytics…"
          : "Opening the text…",
  );
</script>

{#if session}
  {#if session.showFirstRun}
    <!-- First launch: the welcome owns the screen, straight off the loader —
         the reader mounts only after a path is chosen (no John 3 flashing
         under a question, feedback 2026-07-26). -->
    <div class="firstrun-stage">
      <FirstRun />
    </div>
  {:else}
    <Shell />
    <!-- Also mounted over the app: the welcome is re-openable from the top
         bar, and it renders as its own dialog (feedback 2026-07-27). -->
    <FirstRun />
  {/if}
{:else}
  <div class="splash">
    <div class="mark">✦</div>
    <h1>Plumbline</h1>
    <p class="sub">The Holy Bible</p>
    {#if error}
      <!-- The reader gets a sentence they can act on; the RAW string stays one
           disclosure away, because it is what a bug report pastes and the only
           evidence of which rung of the boot ladder broke (audit D-11). -->
      <p class="error">{bootErrorCopy(error)}</p>
      <button onclick={() => location.reload()}>Retry</button>
      <details>
        <summary>Technical details</summary>
        <pre>{error}</pre>
      </details>
    {:else}
      <div class="bar">
        <div
          class="fill"
          class:indeterminate={phase.phase !== "download"}
          style:width={phase.phase === "download" ? `${(phase.fraction ?? 0) * 100}%` : "100%"}
        ></div>
      </div>
      <p class="detail">{phaseLabel}</p>
      {#if phase.phase === "download"}
        <!-- Only while something is actually being downloaded — which, now that
             boot opens in `prepare`, is only ever a cold visit. Saying it on a
             warm boot would be a bill for a purchase already made. -->
        <p class="once">≈3 MB, one time — then Plumbline works with no connection</p>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .firstrun-stage {
    position: fixed;
    inset: 0;
    background: var(--paper, #fcf9f4);
  }
  .splash {
    /* A system serif ON PURPOSE, not an oversight. This screen exists to say
       "we are working on it", and it inherited EB Garamond from body — which is
       render-blocking, so with font-display: block the splash painted NOTHING
       until 1.6 MB of font arrived. Asking for a face that is already on the
       device means it appears immediately, and nothing swaps under the reader
       here (the reader itself is a canvas, painted after the font resolves). */
    font-family: Georgia, "Times New Roman", serif;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    /* EVERY COLOUR HERE IS THE PALETTE'S, not a literal (audit D-11). These were
       the light theme's hexes, so a dark-theme reader got a full-screen cream
       flash on every launch — warm boots included — before `applyTheme()`
       arrived with the truth. The variables are set in index.html's head, from
       last session's stored palette, BEFORE the first paint; the fallbacks after
       the comma are the light theme's own (crates/core/src/theme.rs) and only
       ever apply if that inline block has been removed. */
    background: var(--paper, #fcf9f4);
    color: var(--ink, #211f1a);
  }
  .mark {
    font-size: 28px;
    color: var(--gold, #7d632c);
  }
  h1 {
    font-weight: 500;
    font-size: 30px;
    letter-spacing: 0.04em;
  }
  .sub {
    color: var(--faded, #6c665d);
    font-style: italic;
  }
  .bar {
    width: min(340px, 70vw);
    height: 5px;
    margin-top: 18px;
    border-radius: 3px;
    background: var(--rule, #d8cba8);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--gold, #7d632c);
    border-radius: 3px;
    transition: width 0.15s ease;
  }
  .fill.indeterminate {
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 0.45;
    }
    50% {
      opacity: 1;
    }
  }
  .detail {
    font-size: 13px;
    color: var(--faded, #6c665d);
  }
  /* Quieter than the phase line above it: it is reassurance, not progress. */
  .once {
    font-size: 12px;
    color: var(--faded, #6c665d);
    opacity: 0.85;
    max-width: 32em;
    text-align: center;
    padding: 0 16px;
  }
  .error {
    color: var(--tierResearch, #b04a3a);
    max-width: 40em;
    text-align: center;
    padding: 0 16px;
  }
  .splash button {
    margin-top: 8px;
    padding: 6px 18px;
    border: 1px solid var(--gold, #7d632c);
    border-radius: 6px;
    color: var(--gold, #7d632c);
  }
  /* The raw string, one disclosure away. Monospace and scrollable because it is
     meant to be READ and COPIED into a bug report, not skimmed. */
  details {
    max-width: min(48em, calc(100vw - 32px));
    font-size: 12px;
    color: var(--faded, #6c665d);
  }
  summary {
    cursor: pointer;
  }
  details pre {
    margin-top: 6px;
    max-height: 8em;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    user-select: text;
  }

</style>
