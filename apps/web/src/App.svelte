<script lang="ts">
  // Boot: the engine worker does everything — pack fetch, home, wasm, open, warm,
  // and later the deferred R&D pack — while this thread paints the splash from its
  // progress messages. Fonts load here too, for painting; the worker loads its own
  // copy for layout measurement.
  import { bootErrorCopy } from "./engine/bootError";
  import { deviceLocale, lastLang, setCatalog, t, readerFace } from "./lib/i18n.svelte";
  import { DEFAULT_FONT, FONT_CSS_FAMILY, FONT_FILES } from "./engine/fonts.generated";
  import { EngineRpc, type WorkerProgress } from "./engine/worker-client";
  import { churchFromQuery, hasChurch, launchDestination, sharedAtRef, startsAsNewBeliever } from "./shell/church";
  import { initSession, type Session } from "./state/session.svelte";
  import { dispatchLink } from "./study/links";
  import FirstRun from "./shell/FirstRun.svelte";
  import Shell from "./shell/Shell.svelte";

  // "prepare", not "download": this is what the splash paints before the worker
  // has said anything, and a warm boot downloads nothing.
  let phase = $state<WorkerProgress>({ phase: "prepare" });
  let error = $state<string | null>(null);
  let session = $state<Session | null>(null);

  // The only reactive writers of the document's appearance: state → derived →
  // writer, in that direction only. Two effects, not one, because their inputs
  // differ — the chrome moves whenever Present or Sing opens (they paint their own
  // fixed-light paper over the status bar) and the palette does not, so folding
  // them together would rewrite thirty custom properties and re-stringify the
  // palette into localStorage on every presentation.
  $effect(() => {
    session?.applyTheme();
  });
  $effect(() => {
    session?.applyChrome();
  });

  /** A refKey → the core's `go:` verb, split on the LAST space — the same rule as
   *  core `go_uri` and `VRef::parse_ref_key`, whose contract lets a book id hold a
   *  space. Duplicated in MemorizeHost and StudyPanel. */
  const goUri = (refKey: string): string => `go:${refKey.replace(/ (?=\S*$)/, ":")}`;

  // ── the last net ────────────────────────────────────────────────────────────
  // The global handler for faults nothing else surfaces — a throw in an effect, a
  // component that failed to render, an unawaited rejection — so the reader is told
  // rather than left with a screen that quietly stopped answering. It swallows
  // nothing (no `preventDefault`), so the console and every existing catch still
  // see what they saw before.
  let mishap = $state<string | null>(null);
  /** Whether the bar has already been raised (or refused) this session. */
  let mishapSpent = false;
  /** The document is going away: the rejections that follow a reload or a close are
   *  teardown, not a fault — and the likeliest false positive this bar has, since
   *  Retry and the update notice both reload on purpose. */
  let leaving = false;
  addEventListener("pagehide", () => (leaving = true));

  /** Raise the bar for a fault nothing else surfaced.
   *
   *  Once per session: faults arrive in storms (a render that throws throws again
   *  on every reactive pass), and there is only one remedy to offer anyway.
   *  Dismissal is final for the same reason. Never while the splash is up, which is
   *  exactly `session === null` — boot failure has its own screen with the same
   *  remedy. `error` is deliberately not tested: alongside a live session it can
   *  only come from a throw in the tail of `start()`, which is worth reporting. */
  function noteMishap(detail: string): void {
    if (mishapSpent || leaving || !session) return;
    mishapSpent = true;
    mishap = detail;
  }

  /** The one fault this net deliberately drops: Chromium dispatches a window
   *  `error` reading "ResizeObserver loop completed with undelivered
   *  notifications" whenever an observer callback runs out of passes. It is a
   *  notice, not a failure, and this shell runs three observers. */
  const BENIGN = /^ResizeObserver loop/;

  addEventListener("error", (e: ErrorEvent) => {
    if (BENIGN.test(e.message ?? "")) return;
    noteMishap(e.error instanceof Error ? `${e.error.name}: ${e.error.message}` : e.message || "error");
  });
  addEventListener("unhandledrejection", (e: PromiseRejectionEvent) => {
    const r = e.reason;
    noteMishap(r instanceof Error ? `${r.name}: ${r.message}` : String(r));
  });

  /** The scripture face this device used last launch, written by
   *  `session.applyFonts()`. Only this thread can read localStorage, and it is all
   *  there is before the worker has opened the config; an unknown token falls back
   *  to the shipped default. */
  function hintedTextFont(): string {
    try {
      const token = localStorage.getItem("plumbline:textFont");
      if (token && FONT_FILES[token]) return token;
    } catch {
      /* blocked storage: the default is a Bible either way */
    }
    return DEFAULT_FONT;
  }

  /** The document's own copy of a family, so the canvas paints the real face
   *  rather than the fallback it would otherwise measure once and cache. */
  function documentFaces(token: string): Promise<unknown>[] {
    const fam = FONT_CSS_FAMILY[token] ?? FONT_CSS_FAMILY[DEFAULT_FONT];
    const loads = [document.fonts.load(`18px "${fam}"`), document.fonts.load(`bold 18px "${fam}"`)];
    if (FONT_FILES[token]?.italic) loads.push(document.fonts.load(`italic 18px "${fam}"`));
    return loads;
  }

  /** How many faces the worker should have for a family — 1 for a face with no
   *  italic (Fira Code), 2 otherwise. */
  function expectedFaces(token: string): number {
    return FONT_FILES[token]?.italic ? 2 : 1;
  }

  async function start(): Promise<void> {
    try {
      const rpc = new EngineRpc();
      rpc.onProgress = (p) => (phase = p);
      // Phones defer the machine-tier auto-download; the shell offers an explicit
      // action instead of spending the download behind the reader's back.
      const deferRnd = matchMedia("(max-width: 700px)").matches;
      const hinted = hintedTextFont();
      const [info] = await Promise.all([
        // `textFont` is a hint from localStorage, like `lang`: the real choice is
        // in a config only the worker can read, but the worker needs a face before
        // the first layout. Guess, overlap the download with boot, and reconcile
        // below — a wrong guess costs one relayout before anything is painted.
        rpc.boot({ deferRnd, locale: deviceLocale(), lang: lastLang(), textFont: hinted }),
        ...documentFaces(hinted),
      ]);
      // The worker measures layout with its own FontFaceSet. If its load failed it
      // would silently measure platform-serif metrics while this thread paints real
      // Garamond, and lines would wrap where they are not drawn — so say so.
      if (info.fontFaces !== expectedFaces(hinted)) {
        console.warn(
          `[plumbline] engine worker loaded ${info.fontFaces}/${expectedFaces(hinted)} reader faces — ` +
            `layout is being measured with fallback metrics`,
        );
      }
      // Before the shell mounts: the guessed splash language (i18n.svelte.ts
      // `seed`) is replaced by the one the core resolved from the config.
      setCatalog(info.i18n);
      // The palettes ride on the boot reply: the engine is one worker thread, so
      // carrying the three compiled-in colour tables there saves three queue hops
      // on the one path nothing else can proceed without (BOOT_READS in
      // engine/worker-client.ts).
      const s = initSession(rpc, info, info.palettes ?? {}, info.bundledOn);
      // Both type axes, from the config the worker just handed over — before the
      // shell mounts, so a wrong guess above has painted nothing.
      s.applyFonts();
      // Compared through `readerFace`, because the catalogue has arrived by here:
      // an Arabic session's face is the script one whatever the config says.
      // Without resolving on this side, config === hint and the mismatch is
      // invisible — the engine measures Arabic at a Latin face's optical scale for
      // the whole session.
      if (readerFace(s.config.textFont ?? DEFAULT_FONT) !== hinted) {
        await s.setTextFont(s.config.textFont ?? DEFAULT_FONT);
      }
      // A shared link can carry the sender's church. Save it as this reader's
      // own (theirs wins if they've already set one), then strip it from the
      // address bar so a bookmark or a reload isn't a link about a church.
      const shared = churchFromQuery(location.search);
      s.startAsNewBeliever = startsAsNewBeliever(location.search);
      if (shared && !s.config.church?.name) {
        // Through `setChurch`, not by assigning `config.church`: the meeting time
        // is stored separately in `config.sundayService`, and only setChurch knows
        // to write it.
        s.setChurch(shared);
        s.sharedByChurch = shared;
      } else if (shared) {
        s.sharedByChurch = shared; // shown in the welcome, not saved over theirs
      }
      // A shared passage opens where it points (`?at=Ps 23:1`) — the QR on the
      // Present end card hands over the weave, not just the app.
      const at = sharedAtRef(location.search);
      // A launcher shortcut names a destination (`?open=review`, from the
      // manifest's `shortcuts`). Stripped with the rest: a destination is a way in,
      // and a reload should reopen the reader, not the drill.
      const opened = launchDestination(location.search);
      if (shared || s.startAsNewBeliever || at || opened) {
        history.replaceState(null, "", location.pathname + location.hash);
      }
      // Returning readers never see the welcome, so without this a link's
      // church would be saved with no sign it happened.
      if (hasChurch(shared) && !s.showFirstRun) {
        s.showToast(t("shell.homeChurchSet", { church: shared.name }));
      }
      // "Deferred" must mean the reader wants the machine tier and its download was
      // held back, not merely that no download happened: the tiers are opt-in, so
      // without the last clause every phone would be offered a tier its reader had
      // never asked for.
      s.rndDeferred = deferRnd && !info.rndAuto && s.config.machineAnalysis === true;
      // The TOC, seeded into the read-through cache — not a round trip: it came
      // back on the boot reply (BOOT_READS, engine/worker-client.ts), so this is a
      // local write under the key `q("toc")` reads. Awaited because the two lines
      // below need a canon to clamp and route against.
      await s.fetchQ("toc");
      // `canonSegments` is deliberately not asked for here: only the chrome reads
      // it, through `q()`, which fetches on first render and repaints when the
      // answer lands — awaiting it would put a barrier in front of the text.

      // An incoming address beats the restored position. After the TOC, so a hash
      // naming a book nobody has falls through to the restored session instead of
      // opening a pane on nothing; before the shell mounts, so a routed arrival
      // never flashes last session's chapter first. `history: false`, because the
      // reader never saw the restored chapter.
      //
      // Arrivals only. A reload's address is the one this app stamped last session,
      // while the config underneath it may have been replaced: restoring a backup
      // mutes every config write and reloads, so honouring the address there would
      // open the old session's chapter instead of the backup's.
      const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
      // No entry at all (an engine without the API) is treated as an arrival: a
      // shared link that opens nowhere is the worse of the two failures.
      const routed = nav?.type === "reload" ? null : s.routeFromHash(location.hash);
      if (routed) s.navigate(0, routed.book, routed.chapter, null, { history: false });
      s.installRouter();
      session = s;
      // A worker death after boot is not a window `error`, so the net above cannot
      // see it. Attached here, after the splash has handed over, so a death during
      // boot stays the splash's to report and is never told twice.
      rpc.onFatal = (e) => noteMishap(e.message);
      // After the TOC, so navigation clamps against a real canon, and after the
      // hash: `?at=` names a verse, so it wins when a link carries both.
      if (at) void dispatchLink(s, goUri(at));
      // The shortcut's destination opens on top of the restored reader, using the
      // same states the bottom nav sets, so the Read tab and Back dismiss it the
      // same way.
      if (opened === "hymnal") s.screen = "hymnal";
      else if (opened) {
        s.screen = "memorize";
        s.memorize = { view: opened === "review" ? "review" : "hub" };
      }
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
        // The launch badge: cards that fell due since last session. At idle because
        // it is chrome, not text.
        s.refreshAppBadge();
      });
      // An installed PWA is resumed far more often than launched, so returning to
      // the foreground is the moment worth re-checking (the check throttles itself).
      document.addEventListener("visibilitychange", () => {
        if (document.visibilityState === "visible") {
          void s.checkForUpdate();
          // Resume crosses midnights: a card can have fallen due while the app sat
          // in the background.
          s.refreshAppBadge();
        }
      });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  start();

  // The address bar follows pane 0. An effect rather than a line inside
  // `navigate`, because a pane's chapter moves down half a dozen paths and the URL
  // has to follow all of them, not the ones somebody remembered to instrument.
  $effect(() => {
    session?.syncUrl();
  });

  // The phone's Back button, wired to the surface stack: an open surface owns a
  // history entry, so Back closes it instead of exiting an installed PWA.
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
  <!-- Outside the session block: whatever broke may be the thing that renders the
       app, so this must not be nested inside it. -->
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
    <!-- First launch: the welcome owns the screen; the reader mounts only after a
         path is chosen, so nothing flashes under the question. -->
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
      <!-- The reader gets a sentence they can act on; the raw string stays one
           disclosure away, because it is what a bug report pastes. -->
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
        <!-- Only while something is actually being downloaded — since boot opens
             in `prepare`, only ever a cold visit. -->
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
    /* A system serif on purpose: EB Garamond, inherited from body, is
       render-blocking, so with font-display: block the splash painted nothing
       until 1.6 MB of font arrived. A face already on the device appears at once,
       and nothing swaps here (the reader is a canvas, painted after the font
       resolves). */
    font-family: Georgia, "Times New Roman", serif;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    /* Every colour here is the palette's, not a literal: hard-coded light hexes
       gave a dark-theme reader a full-screen cream flash on every launch, warm
       boots included. The variables are set in index.html's head from last
       session's stored palette, before the first paint; the fallbacks are the
       light theme's own (crates/core/src/theme.rs). */
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
  /* The raw string: monospace and scrollable, meant to be copied into a bug
     report rather than skimmed. */
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

  /* ── the global failure bar ──────────────────────────────────────────────── */
  /* A top bar, where the shell's two sticky notices are bottom ones, so it can
     never land on the notice about the reader's unsaved work. Above the app's
     z-index ceiling of 60. */
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
    /* An installed PWA draws under the status bar (viewport-fit=cover), so this
       inset is the difference between a readable bar and one behind the clock.
       Through the shared variable, not `env()` directly: app.css is the only place
       that writes `env()`, which is what lets a headless browser drive the insets. */
    padding: calc(8px + var(--safeTop, 0px)) 12px 8px;
    background: var(--ink, #211f1a);
    color: var(--paper, #fcf9f4);
    border-bottom: 4px solid var(--tierResearch, #b04a3a);
    /* Scaled like the rest of the chrome. The splash is not, and correctly so: it
       paints before Shell mounts, so `--uiScale` has not been published yet. */
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
