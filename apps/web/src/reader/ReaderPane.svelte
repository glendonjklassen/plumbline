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
  import { hitTest, MARGIN, paintChapter, verseExtents, type LayoutItem, type PaintOverlays } from "./paint";

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
  const bookName = $derived(toc?.books?.find((b: any) => b.id === pane.book)?.name ?? pane.book);

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
    // Re-lay when the WORDS change, not just the geometry — the AKJV overlay
    // swaps them engine-side, so nothing about this pane's own inputs moves.
    // An epoch rather than the setting itself, so the re-layout happens strictly
    // after the engine has been told (see Session.setAkjvOverlay).
    void s.layoutEpoch;
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

  // ── the text mirror ──
  // A canvas holds no text, so to a screen reader, to the browser's own Ctrl+F,
  // and to a translate feature the chapter simply was not there. This mirrors the
  // display list into real DOM text — visually hidden, but present in the
  // accessibility tree and findable — so the words on the page are words the page
  // actually has. Verse by verse, in reading order, so it can be navigated and
  // quoted rather than being one undifferentiated blob.
  //
  // Derived from `items` ALONE: it is rebuilt once per layout and never touches
  // pane.scrollY, so nothing about it sits on the scroll or paint path. A
  // geometry-only re-layout (resize, zoom, spacing) yields the identical strings,
  // and the keyed each below then writes nothing to the DOM at all.
  const mirror = $derived.by(() => {
    const verses: { n: number; text: string }[] = [];
    let cur: { n: number; text: string } | null = null;
    for (const it of items) {
      if (it.kind === "verseNumber") {
        if (it.verseNumber === null) continue;
        cur = { n: it.verseNumber, text: "" };
        verses.push(cur);
      } else if (cur) {
        // A word's text is already pre+word+post, and the canvas sets one space
        // between the boxes — so the mirror joins them the same way.
        cur.text += cur.text ? ` ${it.text}` : it.text;
      }
    }
    return verses;
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
    trackReached();
  }

  /** The reading map's high-water mark: the deepest verse whose text has come
   *  fully into view. Only ever rises within a chapter — reading back up does not
   *  un-read anything, and reporting a fall would put pointless writes on the
   *  scroll path. A verse counts once its LAST word is above the fold. */
  function trackReached(): void {
    if (!items.length || cssH <= 0) return;
    const bottom = pane.scrollY + cssH - MARGIN;
    let deepest = 0;
    for (const it of items) {
      if (it.kind !== "word" || it.y + it.h > bottom) continue;
      const v = it.verse ? Number(it.verse.slice(it.verse.lastIndexOf(":") + 1)) : 0;
      if (v > deepest) deepest = v;
    }
    if (deepest > (pane.reached ?? 0)) pane.reached = deepest;
  }

  // Re-evaluate when a fresh layout lands (a new chapter starts at zero) and
  // when the pane is resized — both change what is above the fold.
  $effect(() => {
    void items;
    void cssH;
    trackReached();
  });
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
    void noteVerses;
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

  // ── touch: tap, long-press menu, horizontal chapter swipe; mouse click.
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
    }
  }
  function onPointerMove(e: PointerEvent): void {
    if (e.pointerType === "touch") {
      touchDx = e.clientX - touchStartX;
      if (Math.abs(e.clientY - touchStartY) > 8 || Math.abs(touchDx) > 8) {
        moved = true;
        if (longPress) clearTimeout(longPress);
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
      {bookName}
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
  <!-- Named so a screen reader can list this pane and jump to it by passage;
       two panes are two regions, "Genesis 1" and "John 3". -->
  <div
    class="scroll"
    bind:this={container}
    onscroll={onScroll}
    title={hoverTitle}
    role="region"
    aria-label={`${bookName} ${pane.chapter}`}
  >
    <div class="spacer" style:height={`${spacerH}px`}>
      <canvas
        bind:this={canvas}
        style:height={`${cssH}px`}
        aria-hidden="true"
        onclick={onClick}
        oncontextmenu={onContextMenu}
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointercancel={onPointerCancel}
        onmousemove={onMouseMove}
      ></canvas>
    </div>
    <!-- The chapter as text. The canvas above is a picture of these words, which
         is why it is aria-hidden: one pane must report its chapter once. -->
    <div class="mirror">
      {#each mirror as v (v.n)}
        <p data-verse={v.n}>{v.n} {v.text}</p>
      {/each}
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
  /* Sized for a thumb, not for a mouse (feedback 2026-07-29: "verse navigation"
     was too small). The passage button is the single most-tapped control in the
     app — it is how a reader gets anywhere — and the chapter arrows either side of
     it were 2px of padding away from being un-hittable on a phone. Android's 48dp
     is the standard both shells now meet. */
  .nav {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: var(--paneNavBg, #efeae1);
    font-size: 16px;
  }
  .nav .passage {
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
    padding: 9px 16px;
    min-height: 44px;
    font-weight: 600;
    font-size: 16.5px;
  }
  .nav button {
    padding: 9px 13px;
    min-height: 44px;
    min-width: 40px;
    border-radius: 6px;
    font-size: 17px;
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
  /* Hidden to the eye, present to everything else. NOT display:none, NOT
     visibility:hidden, NOT aria-hidden — each of those takes the chapter back out
     of the accessibility tree and out of find-in-page, which is the whole bug.
     So: a 1px box that clips its content.

     `position: fixed` rather than absolute so the box is always already in the
     viewport: a Ctrl+F match inside it has nothing to scroll into view, and the
     reader is not thrown to the top of the chapter by finding a phrase in it.
     Fixed also keeps it out of the scroll container's overflow entirely.

     `white-space: nowrap` matters for cost: inside a 1px-wide box, wrapping would
     ask the browser to break a chapter into a couple of thousand line boxes. One
     unwrapped line per verse is a handful of text runs and no line-breaking. */
  .mirror {
    position: fixed;
    top: 0;
    left: 0;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
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
    /* A canvas has no selectable text, but a tap-drag on one can still start a
       selection of the surrounding document — which on a phone shows up as the
       page tinting under your thumb mid-scroll. The tap highlight itself is
       killed globally in app.css. */
    user-select: none;
    -webkit-user-select: none;
  }
</style>
