<script lang="ts">
  // One reading column: nav strip + chapter canvas. Layout comes from the
  // core (display list over the measure callback); this component owns
  // scroll/zoom/gesture state and repaints on any reactive change. The
  // "poll until fresh layout" dance from the desktop shells vanishes here —
  // wasm layout is synchronous.
  import { getSession } from "../state/session.svelte";
  import { fontExtent, measureFor, readerFont } from "./measure";
  import { MARGIN, paintChapter, verseExtents, type LayoutItem, type PaintOverlays } from "./paint";
  import type { DisplayList } from "../engine/StudyEngine";

  const MAX_COLUMN = 720;

  interface Props {
    paneIdx: number;
    onWordStudy?: (refKey: string, tokenIndex: number) => void;
    onWordPin?: (refKey: string, tokenIndex: number) => void;
    overlays?: PaintOverlays;
  }
  let { paneIdx, onWordStudy, onWordPin, overlays = {} }: Props = $props();

  const s = getSession();
  const pane = $derived(s.panes[paneIdx]);

  let container: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let cssW = $state(0);
  let cssH = $state(0);

  let dl: DisplayList | null = null;
  let items = $state<LayoutItem[]>([]);
  let contentH = $state(0);

  const fontPx = $derived(Number(s.config.bodySize ?? 18));
  const sideMargin = $derived(Number(s.config.sideMargin ?? 28));
  const lineSpacing = $derived(Number(s.config.lineSpacing ?? 1.35));
  const versePerLine = $derived(!!s.config.versePerLine);
  const columnWidth = $derived(Math.max(120, Math.min(cssW - 2 * sideMargin, MAX_COLUMN)));
  const marginX = $derived(Math.max(sideMargin, (cssW - columnWidth) / 2));

  const toc = s.engine.toc();
  const chapterCount = $derived(s.engine.chapterCount(pane.book) || 1);

  // Verses in this chapter with weave partners — the gold gutter dot.
  const weaveDots = $derived.by(() => {
    void s.studyEpoch;
    const set = new Set<number>();
    for (const p of s.engine.linkPairs()?.pairs ?? []) {
      if (p.aBook === pane.book && p.aChapter === pane.chapter) set.add(p.aVerse);
      if (p.bBook === pane.book && p.bChapter === pane.chapter) set.add(p.bVerse);
    }
    return set;
  });

  // ── layout: recompute when inputs change ──
  $effect(() => {
    if (!pane || cssW <= 0) return;
    const font = readerFont(fontPx);
    const measure = measureFor(font);
    s.wasm.setMeasure(measure);
    const lineHeight = fontExtent(fontPx) * lineSpacing;
    const next = s.engine.layoutChapter(pane.book, pane.chapter, {
      width: columnWidth,
      lineHeight,
      spaceWidth: measure(" "),
      verseNumGap: measure(" ") * 1.4,
      paraIndent: lineHeight * 0.9,
      paraSpacing: lineHeight * 0.45,
      versePerLine,
    });
    if (!next) return;
    dl?.free();
    dl = next;
    const raw = next.raw as { items: LayoutItem[]; height: number };
    items = raw.items;
    contentH = raw.height;
    // Publish verse-number geometry for the connectors overlay + canon pins.
    const geom = new Map<number, { y: number; h: number }>();
    for (const it of raw.items)
      if (it.kind === "verseNumber" && it.verseNumber !== null && !geom.has(it.verseNumber))
        geom.set(it.verseNumber, { y: it.y, h: it.h });
    s.paneVerseGeom[paneIdx] = geom;
    // Jump to the navigation target (band verse) once the layout is fresh.
    if (pane.targetVerse != null) {
      const e = verseExtents(raw.items).get(pane.targetVerse);
      if (e) pane.scrollY = Math.max(0, e.top - 8);
    }
    clampScroll();
  });

  function maxScroll(): number {
    return Math.max(0, contentH + 2 * MARGIN - cssH);
  }
  function clampScroll(): void {
    pane.scrollY = Math.min(Math.max(pane.scrollY, 0), maxScroll());
  }

  // ── paint on any reactive change ──
  let rafId = 0;
  $effect(() => {
    // Reads register the dependencies; the actual draw is rAF-batched.
    void items;
    void pane.scrollY;
    void s.palette;
    void overlays;
    void weaveDots;
    void cssW;
    void cssH;
    void pane.targetVerse;
    cancelAnimationFrame(rafId);
    rafId = requestAnimationFrame(draw);
  });

  function draw(): void {
    if (!canvas || cssW <= 0) return;
    const dpr = devicePixelRatio || 1;
    if (canvas.width !== Math.round(cssW * dpr) || canvas.height !== Math.round(cssH * dpr)) {
      canvas.width = Math.round(cssW * dpr);
      canvas.height = Math.round(cssH * dpr);
    }
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    paintChapter(
      ctx,
      items,
      {
        palette: s.palette,
        fontPx,
        marginX,
        columnWidth,
        scrollY: pane.scrollY,
        viewportW: cssW,
        viewportH: cssH,
      },
      { bandVerse: pane.targetVerse, weaveDotVerses: weaveDots, ...overlays },
    );
  }

  $effect(() => {
    const ro = new ResizeObserver(() => {
      cssW = container.clientWidth;
      cssH = container.clientHeight;
    });
    ro.observe(container);
    return () => {
      ro.disconnect();
      dl?.free();
      dl = null;
    };
  });

  // ── input ──
  function onWheel(e: WheelEvent): void {
    if (e.ctrlKey) {
      e.preventDefault();
      s.setZoom(fontPx + (e.deltaY < 0 ? 1 : -1));
      return;
    }
    s.activePane = paneIdx;
    const panes = e.shiftKey ? s.panes : [pane];
    for (const p of panes) p.scrollY += e.deltaY;
    clampScroll();
    e.preventDefault();
  }

  function hitAt(e: MouseEvent | PointerEvent): any {
    if (!dl) return null;
    const rect = canvas.getBoundingClientRect();
    return dl.hitTest(e.clientX - rect.left - marginX, e.clientY - rect.top - MARGIN + pane.scrollY);
  }

  // Touch panning; mouse click/dblclick for study + pinning.
  let touchLastY: number | null = null;
  let moved = false;
  function onPointerDown(e: PointerEvent): void {
    s.activePane = paneIdx;
    moved = false;
    if (e.pointerType === "touch") {
      touchLastY = e.clientY;
      canvas.setPointerCapture(e.pointerId);
    }
  }
  function onPointerMove(e: PointerEvent): void {
    if (touchLastY !== null && e.pointerType === "touch") {
      const dy = touchLastY - e.clientY;
      if (Math.abs(dy) > 2) moved = true;
      pane.scrollY += dy;
      clampScroll();
      touchLastY = e.clientY;
    }
  }
  function onPointerUp(e: PointerEvent): void {
    if (e.pointerType === "touch") {
      touchLastY = null;
      if (!moved) {
        const hit = hitAt(e);
        if (hit?.tokenIndex != null) onWordStudy?.(hit.verse, hit.tokenIndex);
      }
      return;
    }
  }
  function onClick(e: MouseEvent): void {
    const hit = hitAt(e);
    if (hit?.tokenIndex == null) return;
    if (e.ctrlKey || e.metaKey) onWordStudy?.(hit.verse, hit.tokenIndex);
    else onWordPin?.(hit.verse, hit.tokenIndex);
  }
  function onDblClick(e: MouseEvent): void {
    const hit = hitAt(e);
    if (hit?.tokenIndex != null) onWordStudy?.(hit.verse, hit.tokenIndex);
  }

  // Hover gloss: native tooltip when the word carries Strong's refs.
  let hoverTitle = $state("");
  function onMouseMove(e: MouseEvent): void {
    const hit = hitAt(e);
    if (hit?.strongs?.length) {
      const st = s.engine.strongs(hit.strongs[0]);
      hoverTitle = st
        ? `${st.code}  ${st.lemma ?? ""}${st.xlit ? `  ${st.xlit}` : ""}\n${(st.kjv || st.def || "").slice(0, 80)}`
        : "";
      canvas.style.cursor = "pointer";
    } else {
      hoverTitle = "";
      canvas.style.cursor = "default";
    }
  }

  const isActive = $derived(s.activePane === paneIdx && s.panes.length > 1);
</script>

<div class="pane" class:active={isActive}>
  <div class="nav">
    <select
      value={pane.book}
      onchange={(e) => s.navigate(paneIdx, (e.target as HTMLSelectElement).value, 1)}
      aria-label="Book"
    >
      {#each toc.books as b (b.id)}
        <option value={b.id}>{b.name ?? b.id}</option>
      {/each}
    </select>
    <button onclick={() => s.stepChapter(paneIdx, -1)} title="Previous chapter">‹</button>
    <input
      type="number"
      min="1"
      max={chapterCount}
      value={pane.chapter}
      onchange={(e) => s.navigate(paneIdx, pane.book, Number((e.target as HTMLInputElement).value))}
      aria-label="Chapter"
    />
    <button onclick={() => s.stepChapter(paneIdx, 1)} title="Next chapter">›</button>
    <span class="spacer"></span>
    {#if s.panes.length < 3}
      <button onclick={() => s.addPane(paneIdx)} title="Split pane">＋</button>
    {/if}
    {#if s.panes.length > 1}
      <button onclick={() => s.closePane(paneIdx)} title="Close pane">✕</button>
    {/if}
  </div>
  <div class="scroll" bind:this={container} title={hoverTitle}>
    <canvas
      bind:this={canvas}
      onwheel={onWheel}
      onclick={onClick}
      ondblclick={onDblClick}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onmousemove={onMouseMove}
    ></canvas>
  </div>
</div>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    border-top: 2px solid transparent;
  }
  .pane.active {
    border-top-color: var(--gold, #9e7d38);
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    background: var(--paneNavBg, #efeae1);
    font-size: 14px;
  }
  .nav select {
    max-width: 11em;
    background: transparent;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 4px;
    padding: 2px 4px;
  }
  .nav input {
    width: 3.2em;
    background: transparent;
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 4px;
    padding: 2px 4px;
    text-align: center;
  }
  .nav button {
    padding: 2px 8px;
    border-radius: 4px;
  }
  .nav button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 14%, transparent);
  }
  .spacer {
    flex: 1;
  }
  .scroll {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    touch-action: none;
  }
</style>
