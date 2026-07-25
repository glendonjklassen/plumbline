<script lang="ts">
  // One reading column: nav strip + chapter canvas. Layout comes from the
  // core (display list over the measure callback); this component owns
  // scroll/zoom/gesture state and repaints on any reactive change. The
  // "poll until fresh layout" dance from the desktop shells vanishes here —
  // wasm layout is synchronous.
  import { getSession } from "../state/session.svelte";
  import { fontExtent, measureFor, readerFont } from "./measure";
  import { itemVerse, MARGIN, paintChapter, verseExtents, type LayoutItem, type PaintOverlays } from "./paint";
  import { highlightTones, nowStamp, type DisplayList } from "../engine/StudyEngine";

  const MAX_COLUMN = 720;

  interface Props {
    paneIdx: number;
    onWordStudy?: (refKey: string, tokenIndex: number) => void;
    overlays?: PaintOverlays;
  }
  let { paneIdx, onWordStudy, overlays = {} }: Props = $props();

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

  const verseNumOf = (refKey: string) => Number(refKey.slice(refKey.lastIndexOf(":") + 1)) || 0;

  // Highlight washes + word-precise runs for this chapter (Tier-0 #4).
  const highlights = $derived.by(() => {
    void s.studyEpoch;
    return s.engine.chapterHighlights(pane.book, pane.chapter);
  });
  const washes = $derived(
    new Map<number, string>((highlights?.verses ?? []).map((v: any) => [verseNumOf(v.verse), v.color])),
  );
  const runs = $derived(
    (highlights?.runs ?? []).map((r: any) => ({ verse: verseNumOf(r.verse), lo: r.lo, hi: r.hi, color: r.color })),
  );

  // Verses with a personal note — the square gutter mark (Tier-0 #3).
  const noteVerses = $derived.by(() => {
    void s.studyEpoch;
    const prefix = `${pane.book} ${pane.chapter}:`;
    const set = new Set<number>();
    for (const n of s.engine.userNotes()?.notes ?? [])
      if (n.verse.startsWith(prefix)) set.add(verseNumOf(n.verse));
    return set;
  });

  // Pinned weave-authoring span, when it belongs to this chapter.
  const pinnedRun = $derived.by(() => {
    const p = pane.pinned;
    if (!p || !p.verse.startsWith(`${pane.book} ${pane.chapter}:`)) return null;
    return { verse: verseNumOf(p.verse), lo: p.lo, hi: p.hi };
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
    void washes;
    void runs;
    void noteVerses;
    void pinnedRun;
    void dragPreview;
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
      {
        bandVerse: pane.targetVerse,
        weaveDotVerses: weaveDots,
        noteVerses,
        washes,
        runs,
        pinned: pinnedRun,
        dragPreview,
        ...overlays,
      },
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

  // ── verse under a point: hit word's verse, else nearest verse-number by y ──
  function verseAt(e: MouseEvent | PointerEvent): string | null {
    const hit = hitAt(e);
    if (hit?.verse) return hit.verse;
    const rect = canvas.getBoundingClientRect();
    const ly = e.clientY - rect.top - MARGIN + pane.scrollY;
    let best: LayoutItem | null = null;
    for (const it of items)
      if (it.kind === "verseNumber" && (!best || Math.abs(it.y + it.h / 2 - ly) < Math.abs(best.y + best.h / 2 - ly)))
        best = it;
    return best?.verseNumber != null ? `${pane.book} ${pane.chapter}:${best.verseNumber}` : null;
  }

  function openContextMenu(clientX: number, clientY: number, e: MouseEvent | PointerEvent): void {
    const refKey = verseAt(e);
    if (refKey) s.contextMenu = { x: clientX, y: clientY, refKey };
  }

  // ── drag highlights (mouse): press pins the start word, a 6px drag
  //    supersedes the pin and previews the range in the last-used tone ──
  const tones: { name: string; hex: string }[] = highlightTones(s.wasm)?.tones ?? [];
  const defaultTone = () =>
    s.lastTone ?? { name: tones[0]?.name.replace(/^./, (c) => c.toUpperCase()) ?? "Amber", hex: tones[0]?.hex ?? "#f6e0a0" };
  let dragStart: { verse: number; tok: number; x: number; y: number } | null = null;
  let dragEnd: { verse: number; tok: number } | null = null;
  let dragPreview = $state<{ verse: number; lo: number; hi: number; color: string }[] | null>(null);

  function maxTokOf(verse: number): number {
    let max = 0;
    for (const it of items)
      if (it.kind === "word" && itemVerse(it) === verse && (it.tokenIndex ?? 0) > max) max = it.tokenIndex ?? 0;
    return max;
  }
  function rangeRuns(a: { verse: number; tok: number }, b: { verse: number; tok: number }, color: string) {
    let [s1, s2] = a.verse < b.verse || (a.verse === b.verse && a.tok <= b.tok) ? [a, b] : [b, a];
    if (s1.verse === s2.verse) return [{ verse: s1.verse, lo: s1.tok, hi: s2.tok, color }];
    const out = [{ verse: s1.verse, lo: s1.tok, hi: maxTokOf(s1.verse), color }];
    for (let v = s1.verse + 1; v < s2.verse; v++) out.push({ verse: v, lo: 0, hi: maxTokOf(v), color });
    out.push({ verse: s2.verse, lo: 0, hi: s2.tok, color });
    return out;
  }

  // ── touch panning + long-press menu + chapter swipe; mouse click/drag ──
  let touchLastY: number | null = null;
  let touchStartX = 0;
  let touchStartY = 0;
  let touchDx = 0;
  let moved = false;
  let longPress: ReturnType<typeof setTimeout> | null = null;
  let suppressClick = false;

  function onPointerDown(e: PointerEvent): void {
    s.activePane = paneIdx;
    moved = false;
    if (e.pointerType === "touch") {
      touchLastY = e.clientY;
      touchStartX = e.clientX;
      touchStartY = e.clientY;
      touchDx = 0;
      canvas.setPointerCapture(e.pointerId);
      const { clientX, clientY } = e;
      longPress = setTimeout(() => {
        if (!moved) {
          openContextMenu(clientX, clientY, e);
          moved = true; // swallow the tap-up
        }
      }, 480);
    } else if (e.button === 0) {
      const hit = hitAt(e);
      if (hit?.tokenIndex != null)
        dragStart = { verse: verseNumOf(hit.verse), tok: hit.tokenIndex, x: e.clientX, y: e.clientY };
    }
  }
  function onPointerMove(e: PointerEvent): void {
    if (touchLastY !== null && e.pointerType === "touch") {
      const dy = touchLastY - e.clientY;
      touchDx = e.clientX - touchStartX;
      if (Math.abs(dy) > 2 || Math.abs(touchDx) > 8) {
        moved = true;
        if (longPress) clearTimeout(longPress);
      }
      pane.scrollY += dy;
      clampScroll();
      touchLastY = e.clientY;
      return;
    }
    if (dragStart && Math.hypot(e.clientX - dragStart.x, e.clientY - dragStart.y) > 6) {
      const hit = hitAt(e);
      if (hit?.tokenIndex != null) {
        dragEnd = { verse: verseNumOf(hit.verse), tok: hit.tokenIndex };
        dragPreview = rangeRuns(dragStart, dragEnd, defaultTone().hex);
      }
    }
  }
  function onPointerUp(e: PointerEvent): void {
    if (longPress) clearTimeout(longPress);
    // Mouse buttons 4/5 → per-pane history (Tier-0 #2).
    if (e.button === 3 || e.button === 4) {
      s.historyStep(paneIdx, e.button === 3 ? -1 : 1);
      return;
    }
    if (e.pointerType === "touch") {
      touchLastY = null;
      // A dominant horizontal fling steps the chapter (Compose parity):
      // left → next, right → previous.
      if (Math.abs(touchDx) > 72 && Math.abs(touchDx) > Math.abs(e.clientY - touchStartY)) {
        s.stepChapter(paneIdx, touchDx < 0 ? 1 : -1);
        touchDx = 0;
        return;
      }
      if (!moved) {
        const hit = hitAt(e);
        if (hit?.tokenIndex != null) onWordStudy?.(hit.verse, hit.tokenIndex);
      }
      return;
    }
    if (dragStart && dragEnd && dragPreview) {
      // Commit the range highlight (endpoints canonicalised).
      const [a, b] =
        dragStart.verse < dragEnd.verse || (dragStart.verse === dragEnd.verse && dragStart.tok <= dragEnd.tok)
          ? [dragStart, dragEnd]
          : [dragEnd, dragStart];
      const tone = defaultTone();
      const mk = (v: number) => `${pane.book} ${pane.chapter}:${v}`;
      const err = s.engine.highlightAdd(tone.name, tone.hex, mk(a.verse), a.tok, mk(b.verse), b.tok, nowStamp());
      if (err) s.showToast(err);
      else s.lastTone = tone;
      suppressClick = true;
    }
    dragStart = null;
    dragEnd = null;
    dragPreview = null;
  }
  function onClick(e: MouseEvent): void {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    const hit = hitAt(e);
    if (hit?.tokenIndex == null) return;
    if (e.ctrlKey || e.metaKey) {
      onWordStudy?.(hit.verse, hit.tokenIndex);
      return;
    }
    // Single click: pin a span for ＋ link — same-verse clicks re-span from
    // the anchor, a different verse resets (manifest §Weave). Authoring is
    // the reader's own data, so pinning is never mode-gated.
    const p = pane.pinned;
    if (p && p.verse === hit.verse) {
      pane.pinned = {
        verse: p.verse,
        anchor: p.anchor,
        lo: Math.min(p.anchor, hit.tokenIndex),
        hi: Math.max(p.anchor, hit.tokenIndex),
      };
    } else {
      pane.pinned = { verse: hit.verse, anchor: hit.tokenIndex, lo: hit.tokenIndex, hi: hit.tokenIndex };
    }
  }
  function onDblClick(e: MouseEvent): void {
    const hit = hitAt(e);
    if (hit?.tokenIndex != null) onWordStudy?.(hit.verse, hit.tokenIndex);
  }
  function onContextMenu(e: MouseEvent): void {
    e.preventDefault();
    s.activePane = paneIdx;
    openContextMenu(e.clientX, e.clientY, e);
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
      oncontextmenu={onContextMenu}
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
