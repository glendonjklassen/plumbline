<script module lang="ts">
  /**
   * Debounce probe for the e2e suite: layouts asked for against resize ticks
   * handed in. Counted, not timed — a millisecond ceiling would be satisfied by
   * the bug. Module-scoped (covers every pane) and kept in production builds,
   * which is what the e2e suite runs.
   */
  interface ResizeProbe {
    /** Engine layout requests this shell issued — all panes, all causes. */
    requests: number;
    /** Observer callbacks reporting a changed width: what the debounce collapses. */
    ticks: number;
    /** Trailing debounce timers that actually fired. */
    timerFires: number;
    /** `timerFires` when the first layout request went out; must be 0, since a cold
     *  boot must not wait 120 ms for its text. Not cleared by `reset()`. */
    firstRequestTimerFires: number | null;
    reset(): void;
  }
  const resizeProbe: ResizeProbe = {
    requests: 0,
    ticks: 0,
    timerFires: 0,
    firstRequestTimerFires: null,
    reset(): void {
      this.requests = 0;
      this.ticks = 0;
      this.timerFires = 0;
    },
  };
  (globalThis as any).__plumblineResize = resizeProbe;
</script>

<script lang="ts">
  // One reading column: nav strip + chapter canvas. Layout comes from the core
  // (display list over the measure callback); this component owns scroll/zoom/
  // gesture state and repaints on any reactive change.
  //
  // Scrolling is native — the canvas sits sticky inside a spacer sized to the
  // laid-out chapter, so momentum, fling and overscroll come free. `pane.scrollY`
  // mirrors scrollTop both ways: onscroll writes it, and external writers
  // (keyboard, navigation, verse targeting) push it back via the guarded effect
  // below.
  import { untrack } from "svelte";
  import { getSession } from "../state/session.svelte";
  import { hitTest, MARGIN, paintChapter, verseExtents, type LayoutItem, type PaintOverlays } from "./paint";
  import { languages, t } from "../lib/i18n.svelte";

  const MAX_COLUMN = 720;
  /** Page-turn mode's guaranteed side gutter: the 44px touch floor, so a
   *  page-turner remote tapping near an edge always has something to press. */
  const PAGE_TURN_MARGIN = 44;

  interface Props {
    paneIdx: number;
    onWordStudy?: (refKey: string, tokenIndex: number, lang?: string) => void;
    overlays?: PaintOverlays;
  }
  let { paneIdx, onWordStudy, overlays = {} }: Props = $props();

  const s = getSession();
  const pane = $derived(s.panes[paneIdx]);

  let container: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let cssW = $state(0);
  let cssH = $state(0);

  // Raw state, not deep: a display list is replaced wholesale and no item is ever
  // edited (`LayoutItem` is readonly field by field), so per-item proxies cost
  // without buying — 2.30 ms proxied against 0.10 ms raw per walk of Psalm 119,
  // and a scroll frame walks the list three times. Reassignment is still tracked,
  // so the `void items` dependencies below still fire.
  let items = $state.raw<readonly LayoutItem[]>([]);
  // Off the returned display list, not the pane's language: the text decides, and
  // a reader whose Arabic download has not landed is looking at the KJV here.
  let itemsRtl = $state(false);
  let contentH = $state(0);
  /** Which chapter `items` describes — the guard against painting one chapter's
   *  text under another's name. */
  let shownKey = "";

  const fontPx = $derived(Number(s.config.bodySize ?? 20));
  const sideMargin = $derived.by(() => {
    const m = Number(s.config.sideMargin ?? 28);
    // Page-turn mode guarantees the tap gutters whatever the slider says.
    return s.config.pageTurn ? Math.max(m, PAGE_TURN_MARGIN) : m;
  });
  const lineSpacing = $derived(Number(s.config.lineSpacing ?? 1.35));
  const versePerLine = $derived(!!s.config.versePerLine);
  // Both default on: an absent key is a config written before the setting existed,
  // not a reader who turned it off.
  const verseNumbers = $derived(s.config.verseNumbers !== false);
  const addedItalics = $derived(s.config.addedItalics !== false);
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


  // Verses with a personal note — the square gutter mark.
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
    // A pane whose text is not open yet must not ask for a layout the worker can
    // only refuse; `langLoading` flipping false re-runs this effect, which paints
    // the pane the moment its engine is ready.
    if (pane.lang && (pane.langLoading || pane.langError)) return;
    // Re-lay when the words change, not just the geometry: the AKJV overlay swaps
    // them engine-side. An epoch, not the setting, so the re-layout happens after
    // the engine has been told (Session.setAkjvOverlay).
    void s.layoutEpoch;
    const seq = ++layoutSeq;
    // A different chapter drops the old display list at once: the nav strip and
    // header change the instant the reader taps, so holding the previous chapter
    // showed John's text under a header reading Acts. A re-layout of the same
    // chapter (resize, zoom, spacing) keeps its text on screen.
    const key = `${pane.book} ${pane.chapter} ${pane.lang ?? ""}`;
    if (key !== shownKey) {
      untrack(() => {
        shownKey = key;
        items = [];
        itemsRtl = false;
        contentH = 0;
        s.paneVerseGeom[paneIdx] = new Map();
      });
    }
    resizeProbe.requests++;
    if (resizeProbe.firstRequestTimerFires === null)
      resizeProbe.firstRequestTimerFires = resizeProbe.timerFires;
    s.rpc
      .layout(pane.book, pane.chapter, {
        font: fontPx,
        width: columnWidth,
        lineSpacing,
        versePerLine,
        verseNumbers,
        lang: pane.lang,
      })
      .then((raw: { items: LayoutItem[]; height: number; rtl?: boolean } | null) => {
        if (seq !== layoutSeq || !raw) return;
        items = raw.items;
        itemsRtl = raw.rtl === true;
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
  // A canvas holds no text, so a screen reader, the browser's Ctrl+F and any
  // translate feature see no chapter at all. This mirrors the display list into
  // real DOM text — visually hidden, present in the accessibility tree — verse by
  // verse in reading order. Derived from `items` alone, never pane.scrollY, so it
  // stays off the scroll and paint path; a geometry-only re-layout yields
  // identical strings and the keyed each below then writes nothing.
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

  // Lay out the neighbouring chapters while the reader reads, so ‹ › and a swipe
  // land on an already-laid-out page. The worker keeps them in its turn cache; the
  // shell never receives the display lists.
  let prefetchTimer: ReturnType<typeof setTimeout> | null = null;
  function prefetchNeighbours(): void {
    if (prefetchTimer) clearTimeout(prefetchTimer);
    const cfg = { font: fontPx, width: columnWidth, lineSpacing, versePerLine, verseNumbers, lang: pane.lang };
    const { book, chapter } = pane;
    const count = s.chapterCount(book);
    prefetchTimer = setTimeout(() => {
      for (const c of [chapter + 1, chapter - 1])
        if (c >= 1 && (count === 0 || c <= count)) void s.rpc.prefetch(book, c, cfg);
    }, 400);
  }

  // Scroll the navigation target into view on each fresh layout, until the reader
  // scrolls this pane themselves or it navigates again. Re-applying per layout
  // holds the verse while pane widths settle (splits, panel open/close, zoom).
  $effect(() => {
    if (!pane.pendingScroll) return;
    void items;
    untrack(() => {
      const e = pane.targetVerse != null ? verseExtents(items).get(pane.targetVerse) : undefined;
      if (e) pane.scrollY = Math.max(0, e.top - 8);
      clampScroll();
    });
  });

  // The top of the chapter's last text line — where overscroll stops. Falls
  // back to the content bottom for a layout with no words (never in practice).
  const lastLineTop = $derived(
    items.reduce((m, it) => (it.kind === "word" && it.y > m ? it.y : m), 0) || contentH,
  );
  function maxScroll(): number {
    // On a phone the reader may keep pushing until the chapter's last LINE reaches
    // the top of the pane, for reading lying down where the bottom of the screen is
    // blocked. The line, not the content bottom, so the text cannot slide off and
    // leave a blank pane; the spacer below carries the same tail, so it is real
    // scroll room with no rubber-band snap-back. Desktops keep the classic stop.
    if (s.narrow) return lastLineTop + MARGIN;
    return Math.max(0, contentH + 2 * MARGIN - cssH);
  }
  function clampScroll(): void {
    // No layout yet: leave pane.scrollY alone — it may hold a restored offset
    // (the boot preview's position) that the first layout will honour.
    if (contentH <= 0) return;
    pane.scrollY = Math.min(Math.max(pane.scrollY, 0), maxScroll());
  }

  // ── native scroll ↔ pane.scrollY ──
  // cssH + maxScroll(), so the browser's own clamp agrees with clampScroll.
  const spacerH = $derived(
    contentH > 0
      ? cssH + (s.narrow ? lastLineTop + MARGIN : Math.max(0, contentH + 2 * MARGIN - cssH))
      : cssH,
  );
  let programmaticScroll = false;
  function onScroll(): void {
    const top = container.scrollTop;
    if (programmaticScroll) {
      programmaticScroll = false;
    } else if (Math.abs(top - pane.scrollY) > 0.5) {
      // The reader scrolled this pane themselves: a pending scroll-to-verse must
      // not fight them.
      pane.pendingScroll = false;
      s.activePane = paneIdx;
      pane.scrollY = top;
      // Chained panes follow — from the user branch only, so a linked move (which
      // arrives with the programmatic flag up) can never echo back.
      s.syncLinkedScroll(paneIdx);
    }
    pane.scrollY = top;
    trackReached();
  }

  /** The reading map's high-water mark: the deepest verse whose last word has come
   *  fully into view. Only rises within a chapter — reading back up un-reads
   *  nothing, and reporting a fall would put writes on the scroll path. */
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
    // Every paint input must be named here: `draw` runs in a rAF callback, outside
    // this effect's tracking scope, so a read down there registers nothing.
    void addedItalics;
    // Clamp before painting; untracked, because clamping must never feed back into
    // layout.
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
        addedItalics,
        rtl: itemsRtl,
      },
      {
        bandVerse: pane.targetVerse,
        weaveDotVerses: weaveDots,
        noteVerses,
        ...overlays,
      },
    );
  }

  // ── resize: measure every tick, re-lay out only when it settles ──
  // A ResizeObserver fires once per frame while a window is dragged or a phone
  // rotates, and `cssW` feeds the layout effect above — so every frame was an
  // engine round trip for the whole chapter, evicting the worker's turn cache
  // (prefetched neighbours included) within an eighth of a second of dragging.
  // Trailing, not leading: the size the reader stopped at is the size the text has
  // to be correct for, and each tick re-arms the timer so the last size wins.
  //
  // `cssW === 0` means no layout has been on screen yet, so the first measurement
  // is never delayed and a cold boot lays out inside the observer's first callback.
  // `cssH` is applied on every tick regardless: it drives the paint, the canvas box
  // and the scroll spacer, not the layout, so holding it back would only letterbox
  // the canvas mid-drag.
  const RESIZE_SETTLE_MS = 120;
  $effect(() => {
    let settle: ReturnType<typeof setTimeout> | null = null;
    let pendingW = 0;
    const ro = new ResizeObserver(() => {
      cssH = container.clientHeight;
      pendingW = container.clientWidth;
      if (settle) clearTimeout(settle);
      settle = null;
      // Also covers a drag that came back to where it started: the layout on
      // screen is already the right one, so there is nothing pending to apply.
      if (pendingW === cssW) return;
      resizeProbe.ticks++;
      if (cssW === 0) {
        cssW = pendingW;
        return;
      }
      settle = setTimeout(() => {
        settle = null;
        resizeProbe.timerFires++;
        cssW = pendingW;
      }, RESIZE_SETTLE_MS);
    });
    ro.observe(container);
    return () => {
      ro.disconnect();
      // A pending settle must not outlive the pane: the timer holds this closure
      // and, through `container`, its detached DOM. Closing a pane resizes the
      // others, so a drag is often in flight at exactly this moment.
      if (settle) clearTimeout(settle);
      settle = null;
    };
  });

  // ── input ──
  // Plain wheel is native. This handler claims only the modified gestures
  // (ctrl+wheel zoom, shift+wheel scroll-all-panes), so it must be a real
  // non-passive listener: Svelte attaches `onwheel` passively, where
  // preventDefault is ignored.
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

  // ── verse under a point: the hit word's verse, else the nearest item on (or
  //    within one line of) the tapped line. Words carry their verse, so the word
  //    beside the tap is always right; beyond a line's height from everything is
  //    padding, and padding is not a verse. ──
  function verseAt(e: MouseEvent | PointerEvent): string | null {
    const hit = hitAt(e);
    if (hit?.verse) return hit.verse;
    const rect = canvas.getBoundingClientRect();
    const lx = e.clientX - rect.left - marginX;
    const ly = e.clientY - rect.top - MARGIN + pane.scrollY;
    let best: LayoutItem | null = null;
    let bestKey = Infinity;
    for (const it of items) {
      const dy = ly < it.y ? it.y - ly : ly > it.y + it.h ? ly - (it.y + it.h) : 0;
      if (dy > it.h) continue; // more than a line away vertically: dead space
      const dx = lx < it.x ? it.x - lx : lx > it.x + it.w ? lx - (it.x + it.w) : 0;
      // Anything on the tapped line beats everything off it; ties by x.
      const key = dy * 10000 + dx;
      if (key < bestKey) {
        bestKey = key;
        best = it;
      }
    }
    if (best?.verse) return best.verse;
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
    // A fresh press clears the swallow: if the tap that set it never produced a
    // click, the stale flag must not eat this genuinely new one.
    suppressClick = false;
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
    // Mouse buttons 4/5 → per-pane history.
    if (e.button === 3 || e.button === 4) {
      s.historyStep(paneIdx, e.button === 3 ? -1 : 1);
      return;
    }
    if (e.pointerType === "touch") {
      if (touchCancelled) return;
      // A dominant horizontal fling steps the chapter, toward the side the text
      // runs: left → next in English, right → next in a right-to-left text.
      if (Math.abs(touchDx) > 72 && Math.abs(touchDx) > Math.abs(e.clientY - touchStartY)) {
        s.stepChapter(paneIdx, touchDx < 0 !== itemsRtl ? 1 : -1);
        touchDx = 0;
        return;
      }
      if (!moved) {
        // The browser re-delivers this tap as a synthesized `click`, hit-tested
        // against the page as it is then. `suppressClick` covers the case where it
        // still targets the canvas; when the tap has opened a dialog, the ghost
        // presses whatever control now sits under the finger — so it is swallowed
        // at the document, wherever it lands.
        suppressClick = true;
        swallowGhostClick();
        if (!pageTurnTap(e)) onTapWord(e);
      }
      return;
    }
  }
  /** Eat the synthesized `click` this touch tap is about to produce, whatever it
   *  targets. Capture-phase so it runs before any handler; `once` plus a 200 ms
   *  fuse so a tap whose ghost never arrives cannot cost a later, real click. */
  function swallowGhostClick(): void {
    const swallow = (e: MouseEvent): void => {
      e.preventDefault();
      e.stopPropagation();
    };
    document.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => document.removeEventListener("click", swallow, true), 200);
  }
  /** Page-turn mode: a tap in the side gutters pages the text (right ahead, left
   *  back) so a page-turner remote can drive it hands-free. 85% of a screen, to
   *  match the keyboard PageDown in Shell.svelte. Returns true when the tap was a
   *  page turn and must not fall through to word study. */
  function pageTurnTap(e: MouseEvent | PointerEvent): boolean {
    if (!s.config.pageTurn) return false;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    if (x >= marginX && x <= marginX + columnWidth) return false;
    // The far side of the column advances — the left margin for a right-to-left
    // text; `settings.pageTurnDesc` says so in each language.
    const forward = x > marginX + columnWidth ? 1 : -1;
    const dir = itemsRtl ? -forward : forward;
    pane.pendingScroll = false;
    pane.scrollY = Math.min(Math.max(0, pane.scrollY + dir * 0.85 * cssH), maxScroll());
    return true;
  }

  // A word tap: in concept-study mode it tags the verse; otherwise it opens word
  // study.
  function onTapWord(e: MouseEvent | PointerEvent): void {
    if (s.inConceptStudy) {
      const refKey = verseAt(e);
      if (refKey) void s.conceptStudyTagVerse(refKey);
      return;
    }
    const hit = hitAt(e);
    if (hit?.tokenIndex != null) onWordStudy?.(hit.verse, hit.tokenIndex, pane.lang);
  }
  // Single click a word (touch taps go through onPointerUp).
  function onClick(e: MouseEvent): void {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    if (pageTurnTap(e)) return;
    onTapWord(e);
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
      // Cache-warmed on first hover; the tooltip fills on the next move. Asked of
      // this pane's text (qIn), so the gloss agrees with the study card a click
      // opens — plain `q` answers from the app-language engine instead.
      const st = s.qIn(pane.lang, "strongs", hit.strongs[0]);
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

  // ── this pane's text language ──
  // The chip names the Bible the column is painting, not the language — Luther,
  // not Deutsch — matching what the study card says about the same choice.
  let langMenu = $state(false);
  const langChoices = $derived(languages());
  const paneBible = $derived.by(() => {
    const own = langChoices.find((l) => l.code === (pane.lang || s.config.language || ""));
    return own?.bible ?? langChoices.find((l) => l.code === "en")?.bible ?? "";
  });
  /** Download progress for THIS pane's language, 0..1 while it is coming. */
  let langFraction = $state<number | null>(null);
  $effect(() => {
    const prev = s.rpc.onPaneLangProgress;
    s.rpc.onPaneLangProgress = (code, fraction) => {
      prev(code, fraction);
      if (pane.langLoading && code === pendingLang) langFraction = fraction;
    };
    return () => {
      s.rpc.onPaneLangProgress = prev;
    };
  });
  let pendingLang = $state("");

  async function pickLang(code: string): Promise<void> {
    langMenu = false;
    pendingLang = code;
    langFraction = null;
    await s.setPaneLang(paneIdx, code);
    pendingLang = "";
    langFraction = null;
  }
</script>

<div class="pane" class:active={isActive}>
  <div class="nav">
    <button onclick={() => s.stepChapter(paneIdx, -1)} title={t("common.previousChapter")}>‹</button>
    <button
      class="passage"
      onclick={() => (s.bookNavFor = paneIdx)}
      title={t("pane.goTo")}
    >
      {bookName}
      {pane.chapter} ▾
    </button>
    <button onclick={() => s.stepChapter(paneIdx, 1)} title={t("common.nextChapter")}>›</button>
    <span class="spacer"></span>
    <!-- Only where there is a choice: one shipped language is no decision. -->
    {#if langChoices.length > 1}
      <button
        class="lang"
        onclick={() => (langMenu = !langMenu)}
        title={t("pane.textLanguage")}
        aria-haspopup="menu"
        aria-expanded={langMenu}
      >
        {paneBible} ▾
      </button>
    {/if}
    <!-- The chain: only where there is a same-chapter pane to chain to. One
         global toggle, not per-pane — a chain with one end is not a chain. -->
    {#if s.panes.some((p, j) => j !== paneIdx && p.book === pane.book && p.chapter === pane.chapter)}
      <button
        class="chain"
        class:on={s.scrollLinked}
        aria-pressed={s.scrollLinked}
        onclick={() => (s.scrollLinked = !s.scrollLinked)}
        title={t("pane.linkScroll")}>⛓︎</button>
    {/if}
    {#if s.panes.length < s.maxPanes}
      <button onclick={() => s.addPane(paneIdx)} title={t("pane.split")}>＋</button>
    {/if}
    {#if s.panes.length > 1}
      <button onclick={() => s.closePane(paneIdx)} title={t("pane.close")}>✕</button>
    {/if}
  </div>
  {#if langMenu}
    <!-- Click-away, so the menu behaves like every other popup in the shell. -->
    <button class="lang-backdrop" onclick={() => (langMenu = false)} aria-label={t("common.close")}></button>
    <div class="lang-menu" role="menu">
      {#each langChoices as l (l.code)}
        <button
          role="menuitem"
          class:on={(pane.lang || "") === (l.code === (s.config.language || "en") ? "" : l.code)}
          onclick={() => void pickLang(l.code === (s.config.language || "en") ? "" : l.code)}
        >
          <span class="bible">{l.bible}</span>
          <span class="endonym">{l.endonym}</span>
        </button>
      {/each}
    </div>
  {/if}
  {#if pane.langLoading}
    <!-- In the pane, not over the app: the column beside this one is still
         being read, and a full-screen overlay would stop it. -->
    <p class="lang-status" role="status">
      {langFraction === null
        ? t("pane.langOpening")
        : t("pane.langDownloading", { percent: Math.round(langFraction * 100) })}
    </p>
  {:else if pane.langError}
    <p class="lang-status error" role="status">{t("pane.langFailed")}</p>
  {/if}
  <!-- Named so a screen reader can list this pane and jump to it by passage. -->
  <div
    class="scroll"
    bind:this={container}
    onscroll={onScroll}
    title={hoverTitle}
    role="region"
    aria-label={`${bookName} ${pane.chapter}`}
  >
    {#if items.length === 0}
      <!-- While the first layout is pending: the engine is one thread and a
           chapter's display list is a round trip — usually one frame, but long
           enough to read as a dead screen when a language switch is decoding a
           corpus at the same time. `aria-hidden` because the region already
           announces its chapter and the mirror below carries the words. -->
      <p class="settling" aria-hidden="true">{t("pane.settling")}</p>
    {/if}
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
    <!-- The chapter as text; the canvas above is a picture of these same words,
         hence aria-hidden — one pane must report its chapter once. -->
    <div class="mirror">
      {#each mirror as v (v.n)}
        <p data-verse={v.n}>{v.n} {v.text}</p>
      {/each}
    </div>
  </div>
</div>

<style>
  .pane {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    border-top: 2px solid transparent;
  }
  .pane.active {
    border-top-color: var(--gold, #9e7d38);
  }
  /* Sized for a thumb, not a mouse: the passage button is the most-tapped control
     in the app, and the chapter arrows either side of it were 2px of padding away
     from being un-hittable on a phone. */
  .nav {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: var(--paneNavBg, #efeae1);
    font-size: calc(16px * var(--uiScale, 1));
  }
  .nav .lang {
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 999px;
    padding: 6px 12px;
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--faded, #6c665d);
  }
  .nav .lang:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 12%, transparent);
  }
  /* The click-away layer: below the menu, above everything else in the pane. */
  .lang-backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: transparent;
  }
  .lang-menu {
    position: absolute;
    z-index: 31;
    inset-inline-end: 8px;
    margin-top: 2px;
    background: var(--popupPaper, #f2eee6);
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 8px;
    padding: 4px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 18%);
    display: flex;
    flex-direction: column;
    min-width: 180px;
  }
  .lang-menu button {
    display: flex;
    justify-content: space-between;
    gap: 14px;
    align-items: baseline;
    padding: 10px 12px;
    border-radius: 6px;
    text-align: start;
    color: var(--ink, #211f1a);
  }
  .lang-menu button:hover {
    background: color-mix(in srgb, var(--gold, #9e7d38) 14%, transparent);
  }
  .lang-menu button.on .bible {
    color: var(--gold, #9e7d38);
    font-weight: 600;
  }
  .lang-menu .endonym {
    color: var(--faded, #6c665d);
    font-size: calc(12px * var(--uiScale, 1));
  }
  .lang-status {
    margin: 0;
    padding: 6px 12px;
    font-size: calc(13px * var(--uiScale, 1));
    color: var(--faded, #6c665d);
    background: var(--paneNavBg, #efeae1);
    border-bottom: 1px solid var(--rule, #d8cba8);
  }
  .lang-status.error {
    color: var(--tierResearch, #aa4838);
  }
  .nav .passage {
    border: 1px solid var(--rule, #d8cba8);
    border-radius: 7px;
    padding: 9px 16px;
    min-height: 44px;
    font-weight: 600;
    font-size: calc(16.5px * var(--uiScale, 1));
  }
  .nav button {
    padding: 9px 13px;
    min-height: 44px;
    min-width: 40px;
    border-radius: 6px;
    font-size: calc(17px * var(--uiScale, 1));
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
    /* Native scroll without the classic scrollbar; the canon strip and the verse
       band carry position instead. */
    scrollbar-width: none; /* Firefox */
  }
  .scroll::-webkit-scrollbar {
    display: none; /* Chromium / WebKit */
  }
  .spacer {
    position: relative;
  }
  /* The mirror: hidden to the eye, present to everything else — so not
     display:none, visibility:hidden or aria-hidden, each of which drops the
     chapter out of the accessibility tree and find-in-page. A 1px clipping box
     instead. `position: fixed` keeps it in the viewport, so a Ctrl+F match has
     nothing to scroll into view and cannot throw the reader to the top of the
     chapter; `white-space: nowrap` stops a 1px-wide box breaking the chapter into
     a couple of thousand line boxes. */
  /* Absolute so it does not move the canvas. */
  .settling {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    margin: 0;
    font-size: calc(15px * var(--uiScale, 1));
    font-style: italic;
    color: var(--faded, #8a8276);
    pointer-events: none;
  }
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
    /* Vertical panning belongs to the browser (momentum for free); we keep taps,
       long-press, and the horizontal chapter swipe. */
    touch-action: pan-y;
    /* A canvas has no selectable text, but a tap-drag can still start a selection
       of the surrounding document — on a phone the page tints under your thumb
       mid-scroll. The tap highlight is killed globally in app.css. */
    user-select: none;
    -webkit-user-select: none;
  }
  .chain.on {
    color: var(--gold, #9e7d38);
    text-shadow: 0 0 1px currentColor;
  }
</style>
