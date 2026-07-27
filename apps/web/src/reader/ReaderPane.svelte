<script lang="ts">
  // One reading column: nav strip + chapter canvas. Layout comes from the
  // core (display list over the measure callback); this component owns
  // scroll/zoom/gesture state and repaints on any reactive change.
  //
  // Scrolling is NATIVE (2026-07-26): the canvas sits sticky inside a spacer
  // sized to the laid-out chapter, and the browser owns the scroll — momentum,
  // fling, and overscroll come free (the hand-rolled 1:1 pointer tracking made
  // the whole app feel dead on phones). `pane.scrollY` mirrors scrollTop both
  // ways: onscroll writes it, and external writers (keyboard, navigation,
  // verse targeting) push it back via the guarded effect below.
  import { untrack } from "svelte";
  import { getSession } from "../state/session.svelte";
  import { hitTest, itemVerse, MARGIN, paintChapter, verseExtents, type LayoutItem, type PaintOverlays } from "./paint";
  import { nowStamp } from "../engine/StudyEngine";

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

  let items = $state<LayoutItem[]>([]);
  let contentH = $state(0);
  /** Which chapter `items` describes — the guard against painting one
   *  chapter's text under another's name. */
  let shownKey = "";

  const fontPx = $derived(Number(s.config.bodySize ?? 18));
  const sideMargin = $derived(Number(s.config.sideMargin ?? 28));
  const lineSpacing = $derived(Number(s.config.lineSpacing ?? 1.35));
  const versePerLine = $derived(!!s.config.versePerLine);
  const columnWidth = $derived(Math.max(120, Math.min(cssW - 2 * sideMargin, MAX_COLUMN)));
  const marginX = $derived(Math.max(sideMargin, (cssW - columnWidth) / 2));

  const toc = $derived(s.q("toc"));

  // Verses in this chapter with weave partners — the gold gutter dot.
  const weaveDots = $derived.by(() => {
    void s.studyEpoch;
    const set = new Set<number>();
    for (const p of s.q("linkPairs")?.pairs ?? []) {
      if (p.aBook === pane.book && p.aChapter === pane.chapter) set.add(p.aVerse);
      if (p.bBook === pane.book && p.bChapter === pane.chapter) set.add(p.bVerse);
    }
    return set;
  });

  const verseNumOf = (refKey: string) => Number(refKey.slice(refKey.lastIndexOf(":") + 1)) || 0;

  // Highlight washes + word-precise runs for this chapter (Tier-0 #4).
  const highlights = $derived.by(() => {
    void s.studyEpoch;
    return s.q("chapterHighlights", pane.book, pane.chapter);
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
    for (const n of s.q("userNotes")?.notes ?? [])
      if (n.verse.startsWith(prefix)) set.add(verseNumOf(n.verse));
    return set;
  });

  // ── layout: recompute when inputs change (async — the worker measures and
  //    lays out off-thread; a stale reply is dropped by the sequence check) ──
  let layoutSeq = 0;
  $effect(() => {
    if (!pane || cssW <= 0) return;
    const seq = ++layoutSeq;
    // Moving to a DIFFERENT chapter drops the old display list at once. The
    // nav strip and header change the instant the reader taps, so holding the
    // previous chapter on the canvas until the layout returns showed John's
    // text under a header reading Acts — which reads as broken (feedback
    // 2026-07-26). A re-layout of the SAME chapter (resize, zoom, spacing)
    // keeps its text on screen: there is nothing stale about it.
    const key = `${pane.book} ${pane.chapter}`;
    if (key !== shownKey) {
      untrack(() => {
        shownKey = key;
        items = [];
        contentH = 0;
        s.paneVerseGeom[paneIdx] = new Map();
      });
    }
    s.rpc
      .layout(pane.book, pane.chapter, {
        font: fontPx,
        width: columnWidth,
        lineSpacing,
        versePerLine,
      })
      .then((raw: { items: LayoutItem[]; height: number } | null) => {
        if (seq !== layoutSeq || !raw) return;
        items = raw.items;
        contentH = raw.height;
        // Publish verse-number geometry for the connectors overlay + canon pins.
        const geom = new Map<number, { y: number; h: number }>();
        for (const it of raw.items)
          if (it.kind === "verseNumber" && it.verseNumber !== null && !geom.has(it.verseNumber))
            geom.set(it.verseNumber, { y: it.y, h: it.h });
        s.paneVerseGeom[paneIdx] = geom;
        untrack(clampScroll);
        untrack(prefetchNeighbours);
      });
  });

  // Lay out the chapters on either side while the reader reads, so ‹ › and a
  // swipe land on an already-laid-out page. Idle work behind the visible
  // chapter, cancelled if the pane moves on first — the worker keeps them in
  // its turn cache, and the shell never receives the display lists.
  let prefetchTimer: ReturnType<typeof setTimeout> | null = null;
  function prefetchNeighbours(): void {
    if (prefetchTimer) clearTimeout(prefetchTimer);
    const cfg = { font: fontPx, width: columnWidth, lineSpacing, versePerLine };
    const { book, chapter } = pane;
    const count = s.chapterCount(book);
    prefetchTimer = setTimeout(() => {
      for (const c of [chapter + 1, chapter - 1])
        if (c >= 1 && (count === 0 || c <= count)) void s.rpc.prefetch(book, c, cfg);
    }, 400);
  }

  // Scroll the navigation target into view on each fresh layout, until the
  // user scrolls this pane themselves (wheel/touch/keys clear pendingScroll)
  // or it navigates again. Re-applying per layout keeps the verse in place
  // while pane widths settle (pane splits, panel open/close, zoom); the band
  // itself (pane.targetVerse) persists until the next navigation regardless.
  $effect(() => {
    if (!pane.pendingScroll) return;
    void items;
    untrack(() => {
      const e = pane.targetVerse != null ? verseExtents(items).get(pane.targetVerse) : undefined;
      if (e) pane.scrollY = Math.max(0, e.top - 8);
      clampScroll();
    });
  });

  function maxScroll(): number {
    return Math.max(0, contentH + 2 * MARGIN - cssH);
  }
  function clampScroll(): void {
    // No layout yet: leave pane.scrollY alone — it may hold a restored offset
    // (the boot preview's position) that the first layout will honour.
    if (contentH <= 0) return;
    pane.scrollY = Math.min(Math.max(pane.scrollY, 0), maxScroll());
  }

  // ── native scroll ↔ pane.scrollY ──
  const spacerH = $derived(Math.max(contentH + 2 * MARGIN, cssH));
  let programmaticScroll = false;
  function onScroll(): void {
    const top = container.scrollTop;
    if (programmaticScroll) {
      programmaticScroll = false;
    } else if (Math.abs(top - pane.scrollY) > 0.5) {
      // The reader scrolled this pane themselves — it owns focus, and any
      // pending scroll-to-verse must not fight them.
      pane.pendingScroll = false;
      s.activePane = paneIdx;
    }
    pane.scrollY = top;
  }
  $effect(() => {
    const y = pane.scrollY;
    void contentH; // re-push once the spacer can actually hold the offset
    if (!container || contentH <= 0) return;
    if (Math.abs(container.scrollTop - y) > 0.5) {
      programmaticScroll = true;
      container.scrollTop = y; // the browser clamps; onScroll reads back truth
    }
  });

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
    void dragPreview;
    void cssW;
    void cssH;
    void pane.targetVerse;
    // Clamp before painting (untracked — clamping must never feed back into
    // layout): covers End-key overshoot, resizes, and content changes alike.
    untrack(clampScroll);
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
    return () => ro.disconnect();
  });

  // ── input ──
  // Plain wheel is native (the container scrolls; onScroll mirrors it). This
  // handler only claims the modified gestures — ctrl+wheel zoom, shift+wheel
  // scroll-all-panes — so it must be a real non-passive listener (Svelte
  // attaches onwheel passively, where preventDefault is ignored).
  function onWheelModifiers(e: WheelEvent): void {
    if (e.ctrlKey) {
      e.preventDefault();
      s.setZoom(fontPx + (e.deltaY < 0 ? 1 : -1));
    } else if (e.shiftKey) {
      e.preventDefault();
      s.activePane = paneIdx;
      for (const p of s.panes) {
        p.scrollY = Math.max(0, p.scrollY + e.deltaY);
        p.pendingScroll = false;
      }
    }
  }
  $effect(() => {
    container.addEventListener("wheel", onWheelModifiers, { passive: false });
    return () => container.removeEventListener("wheel", onWheelModifiers);
  });

  function hitAt(e: MouseEvent | PointerEvent): any {
    const rect = canvas.getBoundingClientRect();
    return hitTest(items, e.clientX - rect.left - marginX, e.clientY - rect.top - MARGIN + pane.scrollY);
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

  // ── drag highlights (mouse): press marks the start word, a 6px drag
  //    previews the range in the last-used tone ──
  const defaultTone = () =>
    s.lastTone ?? {
      name: s.tones[0]?.name.replace(/^./, (c) => c.toUpperCase()) ?? "Amber",
      hex: s.tones[0]?.hex ?? "#f6e0a0",
    };
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

  // ── touch: tap, long-press menu, horizontal chapter swipe; mouse click/drag.
  //    Vertical panning is the browser's (touch-action: pan-y) — when native
  //    scroll claims the gesture we get pointercancel and stand down. ──
  let touchStartX = 0;
  let touchStartY = 0;
  let touchDx = 0;
  let touchCancelled = false;
  let moved = false;
  let longPress: ReturnType<typeof setTimeout> | null = null;
  let suppressClick = false;

  function onPointerDown(e: PointerEvent): void {
    s.activePane = paneIdx;
    moved = false;
    if (e.pointerType === "touch") {
      touchCancelled = false;
      touchStartX = e.clientX;
      touchStartY = e.clientY;
      touchDx = 0;
      const { clientX, clientY } = e;
      longPress = setTimeout(() => {
        if (!moved && !touchCancelled) {
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
    if (e.pointerType === "touch") {
      touchDx = e.clientX - touchStartX;
      if (Math.abs(e.clientY - touchStartY) > 8 || Math.abs(touchDx) > 8) {
        moved = true;
        if (longPress) clearTimeout(longPress);
      }
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
  function onPointerCancel(): void {
    // Native scrolling took the pointer — not a tap, not a long-press.
    touchCancelled = true;
    moved = true;
    if (longPress) clearTimeout(longPress);
  }
  function onPointerUp(e: PointerEvent): void {
    if (longPress) clearTimeout(longPress);
    // Mouse buttons 4/5 → per-pane history (Tier-0 #2).
    if (e.button === 3 || e.button === 4) {
      s.historyStep(paneIdx, e.button === 3 ? -1 : 1);
      return;
    }
    if (e.pointerType === "touch") {
      if (touchCancelled) return;
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
      void s.author("highlightAdd", tone.name, tone.hex, mk(a.verse), a.tok, mk(b.verse), b.tok, nowStamp()).then(
        (err) => {
          if (err) s.showToast(err);
          else s.lastTone = tone;
        },
      );
      suppressClick = true;
    }
    dragStart = null;
    dragEnd = null;
    dragPreview = null;
  }
  // Single click a word → word study (Compose tap parity; touch taps already
  // do this in onPointerUp).
  function onClick(e: MouseEvent): void {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
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
      // Cache-warmed on first hover; the tooltip fills on the next move.
      const st = s.q("strongs", hit.strongs[0]);
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
    <button onclick={() => s.stepChapter(paneIdx, -1)} title="Previous chapter">‹</button>
    <button
      class="passage"
      onclick={() => (s.bookNavFor = paneIdx)}
      title="Go to… (book · chapter · verse)"
    >
      {toc?.books?.find((b: any) => b.id === pane.book)?.name ?? pane.book}
      {pane.chapter} ▾
    </button>
    <button onclick={() => s.stepChapter(paneIdx, 1)} title="Next chapter">›</button>
    <span class="spacer"></span>
    {#if s.panes.length < 3 && !s.narrow}
      <button onclick={() => s.addPane(paneIdx)} title="Split pane">＋</button>
    {/if}
    {#if s.panes.length > 1}
      <button onclick={() => s.closePane(paneIdx)} title="Close pane">✕</button>
    {/if}
  </div>
  <div class="scroll" bind:this={container} onscroll={onScroll} title={hoverTitle}>
    <div class="spacer" style:height={`${spacerH}px`}>
      <canvas
        bind:this={canvas}
        style:height={`${cssH}px`}
        onclick={onClick}
        oncontextmenu={onContextMenu}
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointercancel={onPointerCancel}
        onmousemove={onMouseMove}
      ></canvas>
    </div>
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
  .nav .passage {
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 5px;
    padding: 2px 10px;
    font-weight: 600;
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
    overflow-y: auto;
    overflow-x: hidden;
    /* No scroll chaining into the page; pull-to-refresh stays off the text. */
    overscroll-behavior: contain;
    /* Scroll natively but WITHOUT the classic scrollbar: the page is a canvas
       of typeset scripture, and a grey gutter down the middle of a two-pane
       spread is not what this should look like (feedback 2026-07-26). The
       canon strip and the verse band carry position instead. */
    scrollbar-width: none; /* Firefox */
  }
  .scroll::-webkit-scrollbar {
    display: none; /* Chromium / WebKit */
  }
  .spacer {
    position: relative;
  }
  canvas {
    /* Pinned to the scrollport while the spacer provides the scroll range;
       the paint offsets by pane.scrollY, mirrored from scrollTop. */
    position: sticky;
    top: 0;
    display: block;
    width: 100%;
    /* Vertical panning belongs to the browser (momentum for free); we keep
       taps, long-press, and the horizontal chapter swipe. */
    touch-action: pan-y;
  }
</style>
