<script lang="ts">
  // Boot (TODO #28): the ENGINE WORKER does everything — pack fetch, home,
  // wasm, open, warm, and later the deferred R&D pack — while this thread
  // paints the splash from its progress messages. Fonts load here too, for
  // PAINTING; the worker loads its own copy for layout measurement.
  import { EngineRpc, type WorkerProgress } from "./engine/worker-client";
  import { idbGet } from "./engine/idb";
  import { paintChapter } from "./reader/paint";
  import { initSession, type Session } from "./state/session.svelte";
  import Shell from "./shell/Shell.svelte";

  let phase = $state<WorkerProgress>({ phase: "download", fraction: 0 });
  let error = $state<string | null>(null);
  let session = $state<Session | null>(null);

  // ── the boot snapshot: last session's laid-out chapter paints BEFORE the
  //    engine exists (TODO #28 — never a blank Bible page). The worker's real
  //    layout replaces it the moment the session lands. ──
  let snapshot = $state<any | null>(null);
  let snapCanvas = $state<HTMLCanvasElement | null>(null);
  // The preview is a LIVE mini-reader, not a screenshot (feedback
  // 2026-07-26): it scrolls, resumes at the saved position, and wears the
  // app's own chrome so boot reads as "the app, loading its tools".
  let snapScroll = $state(0);
  let snapScrolled = false;
  const snapPalette = (() => {
    try {
      return JSON.parse(localStorage.getItem("plumbline:palette") ?? "null") ?? {};
    } catch {
      return {};
    }
  })();
  try {
    snapScroll = Number(localStorage.getItem("plumbline:lastScroll") ?? 0) || 0;
  } catch {
    /* top of chapter */
  }

  function snapClamp(): void {
    if (!snapshot) return;
    const max = Math.max(0, snapshot.height + 56 - innerHeight);
    snapScroll = Math.min(Math.max(snapScroll, 0), max);
  }
  function onSnapWheel(e: WheelEvent): void {
    snapScroll += e.deltaY;
    snapScrolled = true;
    snapClamp();
    e.preventDefault();
  }
  let snapTouchY: number | null = null;
  function onSnapPointerDown(e: PointerEvent): void {
    if (e.pointerType === "touch") snapTouchY = e.clientY;
  }
  function onSnapPointerMove(e: PointerEvent): void {
    if (snapTouchY === null || e.pointerType !== "touch") return;
    snapScroll += snapTouchY - e.clientY;
    snapScrolled = true;
    snapTouchY = e.clientY;
    snapClamp();
  }
  function onSnapPointerUp(): void {
    snapTouchY = null;
  }

  void idbGet("cache", "lastLayout")
    .then((bytes) => {
      if (bytes && !session) snapshot = JSON.parse(new TextDecoder().decode(bytes));
    })
    .catch(() => {});

  $effect(() => {
    if (!snapshot || !snapCanvas) return;
    void snapScroll;
    const dpr = devicePixelRatio || 1;
    const w = innerWidth;
    const h = innerHeight - 56; // below the chrome mimic
    if (snapCanvas.width !== Math.round(w * dpr) || snapCanvas.height !== Math.round(h * dpr)) {
      snapCanvas.width = Math.round(w * dpr);
      snapCanvas.height = Math.round(h * dpr);
    }
    const ctx = snapCanvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const paint = () =>
      paintChapter(
        ctx,
        snapshot.items,
        {
          palette: snapPalette,
          fontPx: snapshot.fontPx,
          marginX: Math.max(snapshot.sideMargin ?? 28, (w - snapshot.columnWidth) / 2),
          columnWidth: snapshot.columnWidth,
          scrollY: snapScroll,
          viewportW: w,
          viewportH: h - 3,
        },
        {},
      );
    paint();
    // Repaint once the real Garamond lands (first frames may use the
    // fallback serif — text beats blankness).
    void document.fonts.load(`${snapshot.fontPx}px "EB Garamond"`).then(paint);
  });

  async function start(): Promise<void> {
    try {
      const rpc = new EngineRpc();
      rpc.onProgress = (p) => (phase = p);
      const [info] = await Promise.all([
        rpc.boot(),
        document.fonts.load('18px "EB Garamond"'),
        document.fonts.load('italic 18px "EB Garamond"'),
        document.fonts.load('bold 18px "EB Garamond"'),
      ]);
      // Prime what synchronous readers need on their first frame: the theme
      // palettes, highlight tones, and the TOC/canon shape.
      const [light, dark, night, tones] = await Promise.all([
        rpc.static("themePalette", "light"),
        rpc.static("themePalette", "dark"),
        rpc.static("themePalette", "night"),
        rpc.static("highlightTones"),
      ]);
      const s = initSession(rpc, info, { light, dark, night }, info.bundledOn);
      s.tones = tones?.tones ?? [];
      await Promise.all([s.fetchQ("toc"), s.fetchQ("canonSegments")]);
      // Reading continues exactly where the preview left it — including any
      // scrolling done while the engine was still booting.
      if (snapshot && s.panes[0]?.book === snapshot.book && s.panes[0]?.chapter === snapshot.chapter) {
        if (snapScrolled) {
          s.panes[0].scrollY = snapScroll;
          s.panes[0].pendingScroll = false;
          s.panes[0].targetVerse = null;
        } else if (snapScroll > 0) {
          s.panes[0].scrollY = snapScroll;
          s.panes[0].pendingScroll = false;
        }
      }
      session = s;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  start();

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
  <Shell />
{:else if snapshot && !error}
  <!-- Last session's chapter, painted from the snapshot — readable text in
       the first frames. The strip below says the engine is still coming. -->
  <div class="preview" style:background={snapPalette.paper ?? "#fcf9f4"}>
    <!-- The app's own chrome, mimicked so boot looks like the app. -->
    <header class="mimic" style:background={snapPalette.paneNavBg ?? "#efeae1"} style:color={snapPalette.ink ?? "#211f1a"}>
      <span class="mimic-title">Plumbline</span>
      <span class="mimic-sub" style:color={snapPalette.faded ?? "#8a8276"}>{snapshot.book} {snapshot.chapter}</span>
      <span class="mimic-spacer"></span>
      <span class="mimic-loading" style:color={snapPalette.faded ?? "#8a8276"}>preparing your study tools…</span>
    </header>
    <canvas
      bind:this={snapCanvas}
      onwheel={onSnapWheel}
      onpointerdown={onSnapPointerDown}
      onpointermove={onSnapPointerMove}
      onpointerup={onSnapPointerUp}
    ></canvas>
    <div class="strip" title="Loading">
      <div
        class="strip-fill"
        class:indeterminate={phase.phase !== "download"}
        style:width={phase.phase === "download" ? `${(phase.fraction ?? 0) * 100}%` : "100%"}
      ></div>
    </div>
  </div>
{:else}
  <div class="splash">
    <div class="mark">✦</div>
    <h1>Plumbline</h1>
    <p class="sub">1769 King James Version</p>
    {#if error}
      <p class="error">{error}</p>
      <button onclick={() => location.reload()}>Retry</button>
    {:else}
      <div class="bar">
        <div
          class="fill"
          class:indeterminate={phase.phase !== "download"}
          style:width={phase.phase === "download" ? `${(phase.fraction ?? 0) * 100}%` : "100%"}
        ></div>
      </div>
      <p class="detail">{phaseLabel}</p>
    {/if}
  </div>
{/if}

<style>
  .preview {
    position: fixed;
    inset: 0;
  }
  .preview canvas {
    position: absolute;
    top: 56px;
    left: 0;
    right: 0;
    bottom: 0;
    width: 100%;
    height: calc(100% - 56px);
    touch-action: none;
  }
  .mimic {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 56px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
    border-bottom: 1px solid rgba(138, 130, 118, 0.25);
    box-sizing: border-box;
  }
  .mimic-title {
    font-weight: 600;
    letter-spacing: 0.03em;
  }
  .mimic-sub {
    font-size: 14px;
  }
  .mimic-spacer {
    flex: 1;
  }
  .mimic-loading {
    font-size: 12.5px;
    font-style: italic;
    animation: pulse 1.2s ease-in-out infinite;
  }
  .strip {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    background: rgba(158, 125, 56, 0.15);
  }
  .strip-fill {
    height: 100%;
    background: #9e7d38;
    transition: width 0.15s ease;
  }
  .strip-fill.indeterminate {
    animation: pulse 1.2s ease-in-out infinite;
  }
  .splash {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: #fcf9f4;
    color: #211f1a;
  }
  .mark {
    font-size: 28px;
    color: #9e7d38;
  }
  h1 {
    font-weight: 500;
    font-size: 30px;
    letter-spacing: 0.04em;
  }
  .sub {
    color: #8a8276;
    font-style: italic;
  }
  .bar {
    width: min(340px, 70vw);
    height: 5px;
    margin-top: 18px;
    border-radius: 3px;
    background: #ece5d8;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: #9e7d38;
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
    color: #8a8276;
  }
  .error {
    color: #b04a3a;
    max-width: 40em;
    text-align: center;
  }
  button {
    margin-top: 8px;
    padding: 6px 18px;
    border: 1px solid #9e7d38;
    border-radius: 6px;
    color: #9e7d38;
  }
</style>
