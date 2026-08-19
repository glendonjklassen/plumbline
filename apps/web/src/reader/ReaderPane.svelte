<script module lang="ts">
  /**
   * What the resize path asks the engine for, counted rather than timed.
   *
   * The same device as `__plumblinePaint` in `src/reader/paint.ts`, for the same
   * reason: a ResizeObserver fires once per frame for as long as a window is being
   * dragged, so the only honest test of the debounce below compares the layouts it
   * ASKED FOR against the ticks it was handed. A millisecond ceiling would be
   * satisfied by the bug — the working rules record two tests that passed against
   * exactly what they described.
   *
   * Module-scoped, so the counts cover every pane on screen. Present in
   * production builds on purpose (the e2e suite runs the production bundle): the
   * cost is three integer increments per resize tick, and nothing here sits on the
   * scroll or paint path.
   */
  interface ResizeProbe {
    /** Engine layout requests this shell issued — all panes, all causes. */
    requests: number;
    /** Observer callbacks that reported a CHANGED width: what the debounce is
     *  being asked to collapse. */
    ticks: number;
    /** Trailing debounce timers that actually fired. */
    timerFires: number;
    /** `timerFires` as it stood when the FIRST layout request went out. A cold
     *  boot must not wait 120 ms for its text, so this must be 0. Deliberately
     *  NOT cleared by `reset()` — there is only ever one first layout. */
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
  // One reading column: nav strip + chapter canvas. Layout comes from the
  // core (display list over the measure callback); this component owns
  // scroll/zoom/gesture state and repaints on any reactive change.
  //
  // Scrolling is NATIVE: the canvas sits sticky inside a spacer
  // sized to the laid-out chapter, and the browser owns the scroll — momentum,
  // fling, and overscroll come free (the hand-rolled 1:1 pointer tracking made
  // the whole app feel dead on phones). `pane.scrollY` mirrors scrollTop both
  // ways: onscroll writes it, and external writers (keyboard, navigation,
  // verse targeting) push it back via the guarded effect below.
  import { untrack } from "svelte";
  import { getSession } from "../state/session.svelte";
  import { hitTest, MARGIN, paintChapter, verseExtents, type LayoutItem, type PaintOverlays } from "./paint";
  import { languages, t } from "../lib/i18n.svelte";

  const MAX_COLUMN = 720;

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

  // RAW state, not deep state. A display list is REPLACED WHOLESALE — the two
  // writers below only ever ASSIGN a fresh array (the layout reply, and the
  // empty list on a chapter change), and no code anywhere edits an item — so
  // there is nothing for a per-item, per-field proxy to observe.
  //
  // Deep `$state` gave every item a proxy of its own — 2,643 of them on Psalm
  // 119, counted from the page, not estimated — with a lazily-created signal
  // behind each field those proxies were read for: nine of the twelve on the
  // frame path (x, y, w, h, kind, text, flags, verse, verseNumber), eleven once
  // hover and tap are counted. And a scroll frame walks the whole list THREE
  // times: `trackReached` from onscroll, then `verseExtents` and the text loop
  // inside the paint. Every one of those reads went through a trap to watch for a
  // change that cannot happen. Measured in the page on this list, mean of 20
  // walks reading those nine fields: 2.30 ms proxied against 0.10 ms raw — 23x,
  // and it was being paid three times per frame.
  //
  // `LayoutItem` is `readonly` field by field, so the compiler now holds the
  // invariant that makes this correct rather than merely cheap; `verseExtents`
  // memoizes on the same guarantee, which takes the second of those three walks
  // off the frame entirely.
  //
  // Reassignment is still tracked — raw state is a signal on the VARIABLE — so
  // every `void items` dependency below, and the text mirror, behave exactly as
  // they did. `e2e/reader-perf.spec.ts` counts both.
  let items = $state.raw<readonly LayoutItem[]>([]);
  let contentH = $state(0);
  /** Which chapter `items` describes — the guard against painting one
   *  chapter's text under another's name. */
  let shownKey = "";

  const fontPx = $derived(Number(s.config.bodySize ?? 20));
  const sideMargin = $derived(Number(s.config.sideMargin ?? 28));
  const lineSpacing = $derived(Number(s.config.lineSpacing ?? 1.35));
  const versePerLine = $derived(!!s.config.versePerLine);
  // Both default ON: an absent key is a config written before the setting
  // existed, not a reader who turned it off.
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
    // A pane whose text is not open yet — a restored language pane at boot, or
    // a failed open — must not ask for a layout the worker can only refuse
    // ("the de text is not open on this device"). `langLoading` flipping false
    // re-runs this effect, which is what paints the pane the moment its engine
    // is ready.
    if (pane.lang && (pane.langLoading || pane.langError)) return;
    // Re-lay when the WORDS change, not just the geometry — the AKJV overlay
    // swaps them engine-side, so nothing about this pane's own inputs moves.
    // An epoch rather than the setting itself, so the re-layout happens strictly
    // after the engine has been told (see Session.setAkjvOverlay).
    void s.layoutEpoch;
    const seq = ++layoutSeq;
    // Moving to a DIFFERENT chapter drops the old display list at once. The
    // nav strip and header change the instant the reader taps, so holding the
    // previous chapter on the canvas until the layout returns showed John's
    // text under a header reading Acts — which reads as broken. A re-layout of
    // the SAME chapter (resize, zoom, spacing)
    // keeps its text on screen: there is nothing stale about it.
    const key = `${pane.book} ${pane.chapter} ${pane.lang ?? ""}`;
    if (key !== shownKey) {
      untrack(() => {
        shownKey = key;
        items = [];
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
    const cfg = { font: fontPx, width: columnWidth, lineSpacing, versePerLine, verseNumbers, lang: pane.lang };
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

  // The top of the chapter's last text line — where overscroll stops. Falls
  // back to the content bottom for a layout with no words (never in practice).
  const lastLineTop = $derived(
    items.reduce((m, it) => (it.kind === "word" && it.y > m ? it.y : m), 0) || contentH,
  );
  function maxScroll(): number {
    // A PHONE is not the usual bottom-stop (content bottom meets screen
    // bottom): the reader may keep pushing until the chapter's LAST LINE
    // reaches the TOP of the pane — and no further. For reading on your back,
    // where something blocks the bottom of the screen and turning early means
    // moving your head (maintainer UAT ask, 2026-08-11, phones only
    // 2026-08-12). The LINE, not the content bottom: stopping there keeps the
    // line on screen, where the first cap let the text slide off entirely and
    // left a blank pane. The spacer below carries the same tail, so this is
    // real scroll room — no rubber-band snap-back.
    //
    // On a desktop nobody reads lying down and the tail just looks like a
    // scrollbar lying about how much text is left, so the classic stop rules.
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
  // (Reads `s.narrow` and the layout the same way maxScroll does.)
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
      // The reader scrolled this pane themselves — it owns focus, and any
      // pending scroll-to-verse must not fight them.
      pane.pendingScroll = false;
      s.activePane = paneIdx;
      pane.scrollY = top;
      // Chained panes follow — from the USER branch only, so a linked move
      // (which arrives with the programmatic flag up) can never echo back.
      s.syncLinkedScroll(paneIdx);
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
    // The italics switch is a PAINT input, so it has to be named here like the
    // rest: `draw` runs inside a rAF callback, outside this effect's tracking
    // scope, so reading it down there registers nothing and the page keeps the
    // italics until something else happens to repaint. (Verse numbers need no
    // entry — they change `items`, which is already the first dependency.)
    void addedItalics;
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
        addedItalics,
      },
      {
        bandVerse: pane.targetVerse,
        weaveDotVerses: weaveDots,
        noteVerses,
        ...overlays,
      },
    );
  }

  // ── resize: measure every tick, RE-LAY OUT ONLY WHEN IT SETTLES ──
  // A ResizeObserver fires once per frame for as long as a window is being dragged
  // or a phone is mid-rotation, and `cssW` feeds the layout effect above — so every
  // one of those frames was an engine round trip for the whole chapter. Where the
  // column really changed width (a phone, or any pane under 776 px, below
  // MAX_COLUMN) that was a fresh turn-cache key and a real re-layout, and eight
  // frames — an eighth of a second of dragging — was enough to push every entry out
  // of the worker's cache as it then stood, including the two neighbours this pane
  // had just prefetched, so the page turn after a resize paid full price for
  // chapters the device already had. Where the column was capped and the key
  // repeated, the cost was still Psalm 119's 2,643 items structured-cloned back over
  // postMessage per frame, and the text mirror rebuilt from them.
  //
  // TRAILING, not leading: the size the reader stopped at is the size the text has
  // to be correct for, and a leading-edge layout is a layout for a width that has
  // already gone. Each tick re-arms the timer, so the last size wins.
  //
  // THE FIRST MEASUREMENT IS NEVER DELAYED, and the two cases are told apart by
  // `cssW === 0` — which is precisely "no layout has been on screen yet", the same
  // condition the layout effect itself waits for. A cold boot therefore lays out
  // inside the observer's first callback rather than 120 ms after it; only a change
  // to a width the reader can already see is worth waiting out. (A pane that has
  // been measured at zero — collapsed, or hidden behind a dialog — is in that same
  // state, and rightly lays out at once when it comes back.)
  //
  // `cssH` is applied on EVERY tick regardless: it drives the paint, the canvas
  // box and the scroll spacer, not the layout, so holding it back would letterbox
  // the canvas mid-drag (a blank strip under a growing pane) and save nothing.
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
      // A pending settle must not outlive the pane. Svelte has already destroyed
      // the effects that would react to it, so the write itself wakes nothing —
      // what is left is a timer holding this closure, and through `container` its
      // detached DOM, alive until it fires, plus a state write into a component
      // that no longer exists. Neither is worth keeping (closing a pane while a
      // drag is in flight is exactly how the reader gets both at once: closing one
      // resizes the others).
      if (settle) clearTimeout(settle);
      settle = null;
    };
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

  // ── verse under a point: hit word's verse, else the nearest item on (or one
  //    line from) the tapped line. The old fallback took the nearest verse
  //    NUMBER anywhere in the chapter, by y alone — a tap mid-way through a
  //    long verse sat lines from its own number and next to the following
  //    verse's, so it named a verse the reader never touched; and a tap in the
  //    dead space after the last line named one too. Words carry their verse,
  //    so the word beside the tap is both closer and always right; beyond a
  //    line's height from everything is padding, and padding is not a verse. ──
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
    // A fresh press always clears the swallow: if the tap that set it never
    // produced a click (it landed elsewhere, or the browser dropped it), the
    // stale flag must not eat this genuinely new one.
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
        // The browser will re-deliver this same tap as a synthesized `click`,
        // hit-tested against the page AS IT IS THEN. When it still targets the
        // canvas, `suppressClick` stops the tap running twice; but when the tap
        // has already opened the concept-study confirm, the page under the
        // finger IS that dialog, and the ghost click presses whatever control
        // sits there — measured live: the Cancel button, answering "no" to a
        // question the reader never saw ("tapping a verse does nothing"); a
        // finger's width away it would have been "Tag", a silent yes. So the
        // ghost is swallowed at the document, wherever it lands.
        suppressClick = true;
        swallowGhostClick();
        onTapWord(e);
      }
      return;
    }
  }
  /** Eat the synthesized `click` this touch tap is about to produce, whatever
   *  it targets. Capture-phase so it runs before any handler; `once` plus a
   *  short fuse so a tap whose ghost never arrives cannot cost a later, real
   *  click (the ghost is dispatched with the tap's own input sequence — if it
   *  has not come inside 200 ms, it is not coming). */
  function swallowGhostClick(): void {
    const swallow = (e: MouseEvent): void => {
      e.preventDefault();
      e.stopPropagation();
    };
    document.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => document.removeEventListener("click", swallow, true), 200);
  }
  // A word tap: in concept-study mode it tags the verse (fast concept sweep); the
  // rest of the time it opens word study (Compose tap parity).
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
      // Cache-warmed on first hover; the tooltip fills on the next move. Asked
      // of THIS PANE's text (qIn), so a German pane's gloss agrees with the
      // study card a click opens — `q` alone answered from the app-language
      // engine whatever the pane was reading.
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

  // ── this pane's TEXT language (docs/PER-PANE-LANGUAGE.md) ──
  // The chip names the BIBLE, not the language — "Luther", not "Deutsch". It is
  // the text the column is painting, the reader picked a translation, and it is
  // what the study card says about the same choice.
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
    <!-- Only where there is a choice to make: one shipped language is no
         decision, and a chip that never changes anything is furniture. -->
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
    <!-- The chain: only where there is a same-chapter pane to chain TO — the
         one honest moment for it (maintainer UAT, 2026-08-18: the same chapter
         in two languages should scroll together). One global toggle, not
         per-pane: a chain with one end is not a chain. -->
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
    <!-- IN THE PANE, not over the app: the column beside this one is still
         being read, and a full-screen overlay would stop it. -->
    <p class="lang-status" role="status">
      {langFraction === null
        ? t("pane.langOpening")
        : t("pane.langDownloading", { percent: Math.round(langFraction * 100) })}
    </p>
  {:else if pane.langError}
    <p class="lang-status error" role="status">{t("pane.langFailed")}</p>
  {/if}
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
    {#if items.length === 0}
      <!-- WHILE THE FIRST LAYOUT IS PENDING. The engine lives in one thread and
           a chapter's display list is a round trip, so between mounting and the
           first paint this pane is an empty rectangle. Usually that is one frame
           and nobody sees it; after a language switch the German corpus is being
           decoded at the same time and it is long enough to read as a dead screen.
           `aria-hidden`: the region already announces its chapter, and the mirror
           below carries the words for a screen reader. -->
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
  /* Sized for a thumb, not for a mouse. The passage button is the single
     most-tapped control in the app — it is how a reader gets anywhere — and the
     chapter arrows either side of
     it were 2px of padding away from being un-hittable on a phone. Android's 48dp
     is the standard both shells now meet. */
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
    right: 8px;
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
    text-align: left;
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
    /* Scroll natively but WITHOUT the classic scrollbar: the page is a canvas
       of typeset scripture, and a grey gutter down the middle of a two-pane
       spread is not what this should look like. The
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
  /* Quiet and centred — a note that something is coming, not a spinner that
     implies something is wrong. Absolute so it does not move the canvas. */
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
  .chain.on {
    color: var(--gold, #9e7d38);
    text-shadow: 0 0 1px currentColor;
  }
</style>
