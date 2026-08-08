<script lang="ts">
  // Boot (TODO #28): the ENGINE WORKER does everything — pack fetch, home,
  // wasm, open, warm, and later the deferred R&D pack — while this thread
  // paints the splash from its progress messages. Fonts load here too, for
  // PAINTING; the worker loads its own copy for layout measurement.
  //
  // The splash is a SPLASH. Reading text you can't touch reads as broken, not
  // fast — honest progress beats a decoy, so the work goes into making the wait
  // short rather than disguising it.
  import { bootErrorCopy } from "./engine/bootError";
  import { deviceLocale, lastLang, setCatalog, t } from "./lib/i18n.svelte";
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

  // ── the last net (audit D-12) ───────────────────────────────────────────────
  // There was no global handler anywhere in this product. An exception thrown in
  // an effect, a component that failed to render, a rejected promise nobody
  // awaited — every one of them went to the console of a device that has no
  // console, and the reader was left with a screen that had quietly stopped
  // answering. This says so, and offers the one remedy there is.
  //
  // A NET, not a diagnosis. It deliberately does not swallow anything (no
  // `preventDefault`), so the console, the browser's own reporting and every
  // existing catch still see exactly what they saw before.
  let mishap = $state<string | null>(null);
  /** Whether the bar has already been raised (or refused) this session. */
  let mishapSpent = false;
  /** The document is going away. A reload or a close abandons everything in
   *  flight — an engine read mid-answer, a depot write mid-put — and the
   *  rejections that follow are teardown, not a fault the reader can act on.
   *  They are also the likeliest false positive this bar has, because Retry and
   *  the update notice both reload on purpose. */
  let leaving = false;
  addEventListener("pagehide", () => (leaving = true));

  /** Raise the bar for a fault nothing else surfaced.
   *
   *  ONCE PER SESSION, and that is the whole anti-spam rule. Faults arrive in
   *  storms — a render that throws throws again on every reactive pass, a poll
   *  that rejects rejects again on its next tick — and a bar that re-rendered,
   *  or even re-counted, per event would become the failure. The first one
   *  raises it; the rest go to the console, because the message could not change
   *  anyway: there is exactly one thing to offer.
   *
   *  Dismissal is final for the same reason. Reloading is the remedy, and a
   *  reader who has declined it should not be asked again by the very loop that
   *  is broken.
   *
   *  NOT WHILE THE SPLASH IS UP, which is exactly `session === null`. Boot
   *  failure has its own screen — mapped copy, the raw string behind a details,
   *  and a Retry (D-11) — and a bar over it would be the same news twice, told
   *  worse. (Testing `error` as well would be wrong, not merely redundant: it can
   *  only be set alongside a live session by a throw in the tail of `start()`,
   *  after the shell is up, and that is a fault worth reporting.) */
  function noteMishap(detail: string): void {
    if (mishapSpent || leaving || !session) return;
    mishapSpent = true;
    mishap = detail;
  }

  /** The one fault this net deliberately drops. Chromium dispatches a window
   *  `error` reading "ResizeObserver loop completed with undelivered
   *  notifications" whenever an observer callback runs out of passes — it is a
   *  notice, not a failure, nothing is broken by it, and this shell runs three
   *  observers (the reader pane, the canon strip, the connectors overlay). It is
   *  the classic false positive of every global handler ever written, and a bar
   *  that cries wolf on a window resize is worse than no bar. */
  const BENIGN = /^ResizeObserver loop/;

  addEventListener("error", (e: ErrorEvent) => {
    if (BENIGN.test(e.message ?? "")) return;
    noteMishap(e.error instanceof Error ? `${e.error.name}: ${e.error.message}` : e.message || "error");
  });
  addEventListener("unhandledrejection", (e: PromiseRejectionEvent) => {
    const r = e.reason;
    noteMishap(r instanceof Error ? `${r.name}: ${r.message}` : String(r));
  });

  async function start(): Promise<void> {
    try {
      const rpc = new EngineRpc();
      rpc.onProgress = (p) => (phase = p);
      // Phones defer the machine-tier auto-download: the shell
      // offers an explicit "load analysis" action instead of spending the
      // download and the worker time behind the reader's back.
      const deferRnd = matchMedia("(max-width: 700px)").matches;
      const [info] = await Promise.all([
        rpc.boot({ deferRnd, locale: deviceLocale(), lang: lastLang() }),
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
      // Before ANY of the work below, and well before the shell mounts: this is
      // the point the guessed splash language (i18n.svelte.ts `seed`) is
      // replaced by the one the core resolved from the reader's own setting.
      setCatalog(info.i18n);
      // The palettes RIDE ON THE BOOT REPLY (audit F-11): the engine lives in
      // ONE worker thread, so carrying the three compiled-in colour tables on
      // the reply saves three full queue hops on the single path where nothing
      // else can proceed. See BOOT_READS in engine/worker-client.ts.
      const s = initSession(rpc, info, info.palettes ?? {}, info.bundledOn);
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
      // Present end card hands over the weave, not just the app.
      const at = sharedAtRef(location.search);
      if (shared || s.startAsNewBeliever || at) {
        history.replaceState(null, "", location.pathname + location.hash);
      }
      // Returning readers never see the welcome, so without this a link's
      // church would be saved with no sign it happened.
      if (hasChurch(shared) && !s.showFirstRun) {
        s.showToast(t("shell.homeChurchSet", { church: shared.name }));
      }
      // "Deferred" has to mean the reader WANTS the machine tier and its download
      // was held back — not merely that no download happened. The tiers are
      // opt-in, so those two came apart: with the tier off,
      // `rndAuto` is correctly false on every device, and without the last clause
      // every phone would be shown StudyPanel's "Load analysis" offer for a tier
      // its reader had never asked for.
      s.rndDeferred = deferRnd && !info.rndAuto && s.config.machineAnalysis === true;
      // The TOC, seeded into the read-through cache. NOT a round trip any more:
      // it came back on the boot reply and `rpc.call` hands it straight over
      // (BOOT_READS, engine/worker-client.ts), so this is a local write into the
      // cache under the key `q("toc")` reads — awaited only because the two lines
      // below genuinely need a canon to clamp and route against.
      await s.fetchQ("toc");
      // `canonSegments` IS NOT AWAITED, and is not asked for here at all (audit
      // F-11). Nothing on the path to first text reads it — the canon strip, the
      // passage navigator and the maps do, and all four go through `q()`, which
      // fetches on first render and repaints when the answer lands. Awaiting it
      // here made a read that only the CHROME needs a barrier in front of the
      // TEXT, which is the one thing boot is racing for.

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
      // The worker dying AFTER boot had nowhere to go — worker-death.spec.ts says
      // so in as many words ("the shell has no post-boot fatal UI to show yet…
      // the hook is where that UI will attach"). It is not a window `error`, so
      // the net above cannot see it; it is attached HERE, after the splash has
      // handed over, so a death DURING boot is still the splash's to report and
      // is never told twice.
      rpc.onFatal = (e) => noteMishap(e.message);
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
      ? t("boot.phase.download", { percent: Math.round((phase.fraction ?? 0) * 100) })
      : t(`boot.phase.${phase.phase}`),
  );
</script>

{#if mishap}
  <!-- Outside the session block on purpose: whatever broke may be the thing that
       renders the app, so this must not be nested inside it. -->
  <div class="mishap" role="alert">
    <span class="what">{t("boot.mishap")}</span>
    <button class="act" onclick={() => location.reload()}>{t("boot.reload")}</button>
    <button class="act" onclick={() => (mishap = null)}>{t("boot.dismiss")}</button>
    <details>
      <summary>{t("boot.details")}</summary>
      <pre>{mishap}</pre>
    </details>
  </div>
{/if}

{#if session}
  {#if session.showFirstRun}
    <!-- First launch: the welcome owns the screen, straight off the loader —
         the reader mounts only after a path is chosen (no John 3 flashing
         under a question). -->
    <div class="firstrun-stage">
      <FirstRun />
    </div>
  {:else}
    <Shell />
    <!-- Also mounted over the app: the welcome is re-openable from the top
         bar, and it renders as its own dialog. -->
    <FirstRun />
  {/if}
{:else}
  <div class="splash">
    <div class="mark">✦</div>
    <h1>Plumbline</h1>
    <p class="sub">{t("boot.tagline")}</p>
    {#if error}
      <!-- The reader gets a sentence they can act on; the RAW string stays one
           disclosure away, because it is what a bug report pastes and the only
           evidence of which rung of the boot ladder broke (audit D-11). -->
      <p class="error">{t(bootErrorCopy(error))}</p>
      <button onclick={() => location.reload()}>{t("boot.retry")}</button>
      <details>
        <summary>{t("boot.details")}</summary>
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
        <p class="once">{t("boot.oneTime")}</p>
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

  /* ── the global failure bar (audit D-12) ─────────────────────────────────── */
  /* A TOP bar, where the shell's two sticky notices are bottom ones: this can
     appear at the same time as either, and the one thing it must not do is land
     on top of the notice about the reader's unsaved work. Above everything
     (the app's ceiling is z-index 60) — if this is showing, it is the most
     important thing on the screen. */
  .mishap {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 70;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    /* An installed PWA draws under the status bar (viewport-fit=cover), so the
       inset is the difference between a readable bar and one behind the clock.
       Through the shared variable, not `env()` directly: app.css names the four
       insets once and is the only place that writes `env()`, which is what lets
       a headless browser — which has no notch and never will — drive them. */
    padding: calc(8px + var(--safeTop, 0px)) 12px 8px;
    background: var(--ink, #211f1a);
    color: var(--paper, #fcf9f4);
    border-bottom: 4px solid var(--tierResearch, #b04a3a);
    /* Scaled like the rest of the chrome (D-23). The splash below is NOT, and
       correctly so: it paints before Shell mounts, so `--uiScale` has not been
       published yet and the fallback of 1 is the honest value there. */
    font-size: calc(14px * var(--uiScale, 1));
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.25);
  }
  .mishap .what {
    flex: 1 1 auto;
  }
  .mishap .act {
    padding: 5px 12px;
    border: 1px solid currentColor;
    border-radius: 6px;
    font-size: 14px;
    white-space: nowrap;
  }
  .mishap details {
    flex: 1 1 100%;
    color: inherit;
    opacity: 0.8;
  }
</style>
