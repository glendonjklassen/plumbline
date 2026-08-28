// The reader's chapter painter — the web twin of the GTK/WinUI canvas paint
// (manifest §Reader core): verse numbers bold gold; FLAG_ADDED italic gray;
// FLAG_DIVINE / FLAG_TITLE inks; Strong's underline; search/goto bands,
// word-precise runs, pinned spans, and the weave/note gutter marks.

import { readerFontFamily, readerFontHasItalic, readerFontPx } from "./measure";

export const MARGIN = 28; // top/bottom text margin (manifest constant)

// Token flag bits on a display-list item: this shell's MIRROR of the
// `PLUMBLINE_FLAG_*` #defines in crates/ffi/include/plumbline.h, which cbindgen
// const-folds out of crates/ffi/src/lib.rs where a compile-time assert pins each
// one to the core's own constant. Names and values must match that header
// exactly, and every paint site below tests one of these rather than a number:
// `flag_bits_are_mirrored_by_both_shells` (crates/ffi/src/tests.rs) fails on
// either.
export const FLAG_ADDED = 1;
export const FLAG_DIVINE = 2;
export const FLAG_TITLE = 4;
/** Display only: this word is an AKJV re-rendering, set by the overlay on the
 *  display list on the way past. Never in `kjv.jsonl`. */
export const FLAG_RERENDERED = 16;

/**
 * One positioned box in a chapter's display list.
 *
 * IMMUTABLE, and `readonly` field by field so the compiler says so. A display
 * list is produced whole by the core, handed across from the engine worker, and
 * REPLACED WHOLESALE when anything about the layout changes — no code anywhere
 * edits an item in place. Two things downstream depend on exactly that: the
 * items are held in `$state.raw` (no deep proxy, no signal per field), and
 * `verseExtents` is memoized on the identity of the array (below). Both would be
 * wrong — not merely fast — if an item could change under them, so the rule is
 * enforced here rather than remembered.
 */
export interface LayoutItem {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
  readonly text: string;
  readonly kind: "verseNumber" | "word";
  readonly verse: string | null;
  readonly verseDisplay: string | null;
  readonly tokenIndex: number | null;
  readonly verseNumber: number | null;
  readonly flags: number;
  readonly strongs: readonly string[];
}

export interface PaintOverlays {
  /** Verse number to band (search/goto target) — gold wash over its lines. */
  bandVerse?: number | null;
  /** Verses with search hits — banded like the target (manifest Tier-0 #8). */
  hitVerses?: Set<number>;
  /** Verses with weave partners — gold gutter dot by the verse number. */
  weaveDotVerses?: Set<number>;
  /** Verses with a personal note — square gutter mark left of the dot. */
  noteVerses?: Set<number>;
}

export interface PaintOpts {
  palette: any;
  fontPx: number;
  marginX: number;
  columnWidth: number;
  scrollY: number;
  viewportW: number;
  viewportH: number;
  /** Italicize the KJV's supplied words (`config.addedItalics`, default on).
   *  A PAINT decision only — the engine measures every word upright either
   *  way, so turning italics off cannot invalidate a cached layout. */
  addedItalics?: boolean;
  /** Whether this display list was laid out right to left — straight off the
   *  engine's `DisplayList.rtl`, never worked out here. See the `direction`
   *  note in the text section below for what it changes. */
  rtl?: boolean;
}

/** Verse number of an item (verseNumber items carry it; words via refKey). */
export function itemVerse(it: LayoutItem): number | null {
  if (it.verseNumber !== null) return it.verseNumber;
  if (it.verse) {
    const i = it.verse.lastIndexOf(":");
    if (i >= 0) return Number(it.verse.slice(i + 1)) || null;
  }
  return null;
}

/** Hit-test a layout point against the word rectangles — the TS twin of
 *  plumbline_layout_hit_test_json, so hover/tap never crosses to the worker
 *  (TODO #28). Coordinates are layout-space (caller subtracts margins). */
export function hitTest(
  items: readonly LayoutItem[],
  x: number,
  y: number,
): { verse: string; tokenIndex: number; strongs: readonly string[] } | null {
  for (const it of items) {
    if (it.kind !== "word" || it.verse == null || it.tokenIndex == null) continue;
    if (x >= it.x && x <= it.x + it.w && y >= it.y && y <= it.y + it.h)
      return { verse: it.verse, tokenIndex: it.tokenIndex, strongs: it.strongs ?? [] };
  }
  return null;
}

/** One verse's vertical span in layout coords. Read-only because the map below
 *  is SHARED — by every caller and by every frame of a layout — so an entry one
 *  caller edited would be an entry the next paint believes. */
export interface VerseExtent {
  readonly top: number;
  readonly bottom: number;
  readonly firstY: number;
}
export type VerseExtents = ReadonlyMap<number, VerseExtent>;

/**
 * What the paint path is costing, counted rather than guessed at.
 *
 * This is a diagnostic in the same spirit as `__plumbline` on the session: a
 * handle for the console, and the thing `e2e/reader-perf.spec.ts` measures — the
 * count of per-frame recomputations across a real scroll, instead of a
 * millisecond ceiling that says nothing about why a frame was slow.
 *
 * Present in PRODUCTION builds on purpose. The e2e suite runs the production
 * bundle (`vite preview`), so a counter compiled out of the build could not
 * regression-test the build. A steady-state scroll frame pays two increments,
 * one WeakMap lookup and one identity compare for it.
 *
 * `items` is a WeakRef because a probe on the paint path must never be the reason
 * a chapter the reader has left stays in memory.
 */
export interface PaintProbe {
  /** paintChapter calls — i.e. frames actually drawn. */
  paints: number;
  /** Distinct display lists painted: layouts, as opposed to frames. */
  layouts: number;
  /** verseExtents calls, and how many of them had to do the work. */
  extentsCalls: number;
  extentsComputed: number;
  /** The most recently painted display list, and the extents the paint used —
   *  so a test can re-derive the extents and catch a memo gone stale. */
  items: WeakRef<readonly LayoutItem[]> | null;
  extents: VerseExtents | null;
  /** The body font string the last frame set on the canvas — what a test
   *  measures with to check that painted advance widths still agree with the
   *  worker-measured item rects (e2e/font-face.spec.ts). Carries the face's
   *  optical scale, because it is built on `readerFontPx`. */
  bodyFont: string | null;
  /** Zero the counters. Deliberately keeps `items`/`extents` and the
   *  last-painted identity, so `layouts` after a reset counts the layouts that
   *  arrived AFTER it — which is the budget a scroll test compares against. */
  reset(): void;
}

export const paintProbe: PaintProbe = {
  paints: 0,
  layouts: 0,
  extentsCalls: 0,
  extentsComputed: 0,
  items: null,
  extents: null,
  bodyFont: null,
  reset(): void {
    this.paints = 0;
    this.layouts = 0;
    this.extentsCalls = 0;
    this.extentsComputed = 0;
  },
};
(globalThis as any).__plumblinePaint = paintProbe;

/** Identity of the display list the last frame painted (see `paintProbe`).
 *
 *  WEAK for the same reason `paintProbe.items` is: this is a counter's bookmark,
 *  and a counter must not be why a chapter stays in memory — a pane closed right
 *  after a paint would otherwise pin its whole display list forever. A collected
 *  target derefs to `undefined`, which correctly compares unequal to the list
 *  being painted now, so losing the bookmark costs one extra `layouts` tick and
 *  cannot mis-report a shared layout as a fresh one. */
let lastPainted: WeakRef<readonly LayoutItem[]> | null = null;

/**
 * Per-verse vertical extents, memoized ONCE PER LAYOUT.
 *
 * The key is the display list itself, which works because of the invariant
 * stated on `LayoutItem`: a display list is replaced wholesale, never mutated in
 * place. A changed layout is therefore a DIFFERENT array and cannot collide with
 * a stale entry — there is no invalidation to get wrong, and nothing to keep in
 * step. That is what makes the memo correct rather than merely faster.
 *
 * Weak, so an entry dies with the display list it describes rather than pinning
 * every chapter the reader has visited.
 *
 * `e2e/reader-perf.spec.ts` counts the computations across a real scroll.
 */
const extentsMemo = new WeakMap<readonly LayoutItem[], VerseExtents>();

export function verseExtents(items: readonly LayoutItem[]): VerseExtents {
  paintProbe.extentsCalls++;
  const memo = extentsMemo.get(items);
  if (memo) return memo;
  paintProbe.extentsComputed++;
  const out = new Map<number, { top: number; bottom: number; firstY: number }>();
  for (const it of items) {
    const v = itemVerse(it);
    if (v === null) continue;
    const e = out.get(v);
    if (!e) out.set(v, { top: it.y, bottom: it.y + it.h, firstY: it.y });
    else {
      e.top = Math.min(e.top, it.y);
      e.bottom = Math.max(e.bottom, it.y + it.h);
      e.firstY = Math.min(e.firstY, it.y);
    }
  }
  extentsMemo.set(items, out);
  return out;
}

function withAlpha(hex: string, alpha: number): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha})`;
}

export function paintChapter(
  ctx: CanvasRenderingContext2D,
  items: readonly LayoutItem[],
  o: PaintOpts,
  ov: PaintOverlays,
): void {
  const { palette: p, marginX, columnWidth, scrollY, viewportW, viewportH } = o;
  // The px the glyphs are actually drawn at: the reader's size under the face's
  // optical scale — the SAME number the engine worker measured with, because
  // both come from readerFontPx (see reader/measure.ts). Everything below that
  // is glyph-relative (font strings, gutter-mark baselines, underline depth)
  // uses this, not the raw setting.
  const fontPx = readerFontPx(o.fontPx);
  ctx.fillStyle = p.paper ?? "#fcf9f4";
  ctx.fillRect(0, 0, viewportW, viewportH);

  const yOf = (layoutY: number) => MARGIN + layoutY - scrollY;
  const visible = (top: number, bottom: number) => yOf(bottom) >= 0 && yOf(top) <= viewportH;
  const extents = verseExtents(items);

  paintProbe.paints++;
  if (lastPainted?.deref() !== items) {
    lastPainted = new WeakRef(items);
    paintProbe.layouts++;
    paintProbe.items = lastPainted;
    paintProbe.extents = extents;
  }

  // ── hit / goto bands ──
  const bandRect = (e: { top: number; bottom: number }) =>
    [marginX - 6, yOf(e.top), columnWidth + 12, e.bottom - e.top] as const;

  const gold = p.gold ?? "#9e7d38";
  const bandVerses = new Set<number>(ov.hitVerses ?? []);
  if (ov.bandVerse != null) bandVerses.add(ov.bandVerse);
  for (const v of bandVerses) {
    const e = extents.get(v);
    if (e && visible(e.top, e.bottom)) {
      ctx.fillStyle = withAlpha(gold, 0.12);
      ctx.fillRect(...bandRect(e));
    }
  }

  // ── gutter marks: weave dot + note square beside the verse number ──
  for (const [v, e] of extents) {
    const cy = yOf(e.firstY) + fontPx * 0.55;
    if (cy < -10 || cy > viewportH + 10) continue;
    if (ov.weaveDotVerses?.has(v)) {
      ctx.fillStyle = withAlpha(gold, 0.75);
      ctx.beginPath();
      ctx.arc(marginX - 9, cy, 2.3, 0, Math.PI * 2);
      ctx.fill();
    }
    if (ov.noteVerses?.has(v)) {
      ctx.fillStyle = withAlpha(p.ink ?? "#211f1a", 0.55);
      ctx.fillRect(marginX - 16.5, cy - 2.25, 4.5, 4.5);
    }
  }

  // ── text ──
  ctx.textBaseline = "top";
  // TWO SETTINGS THAT LOOK REDUNDANT AND ARE NOT. Measured in chromium rather
  // than reasoned about, because the first guess here was wrong.
  //
  // textAlign "left", NOT the "start" default. `start` means "whichever edge
  // this context calls the beginning", so under direction:rtl it flips x from
  // the left edge of the box to the right one — measured: origin 200 puts ink at
  // 50..195 instead of 203..348. The engine has already decided where every box
  // goes (it mirrors the whole display list for a right-to-left text), so x here
  // is always a LEFT edge and the canvas must read it that way. With "left"
  // pinned, x IS the left edge under both directions.
  //
  // direction follows the TEXT, and it is not cosmetic. It decides which side a
  // bidi-neutral character lands on, which is every full stop, comma and
  // guillemet in the Van Dyck. Measured on "والأرض." — under ltr the period sits
  // at the RIGHT of the word, which in an Arabic line reads as a full stop
  // leading the sentence; under rtl it sits at the left, where it belongs. The
  // opening guillemet of a quotation is the same fact mirrored.
  //
  // Advance widths do NOT depend on it (153px either way, measured), which is
  // what keeps the engine worker's memo valid across a language switch.
  ctx.textAlign = "left";
  ctx.direction = o.rtl ? "rtl" : "ltr";
  const family = readerFontFamily();
  const bodyFont = `${fontPx}px ${family}`;
  paintProbe.bodyFont = bodyFont;
  // A face with no italic (Fira Code) must not be ASKED for one: the browser
  // would shear the upright, and a fake italic on every translator-supplied word
  // is worse than none. Those words still read as supplied — the palette's
  // `added` tone below carries it, in every face. A reader who turns the
  // italics off lands in exactly that same place, deliberately.
  const italicFont =
    readerFontHasItalic() && o.addedItalics !== false ? `italic ${fontPx}px ${family}` : bodyFont;
  const boldFont = `bold ${fontPx}px ${family}`;
  for (const it of items) {
    if (!visible(it.y, it.y + it.h)) continue;
    const x = marginX + it.x;
    const y = yOf(it.y);
    if (it.kind === "verseNumber") {
      ctx.font = boldFont;
      ctx.fillStyle = gold;
      ctx.fillText(it.text, x, y);
      continue;
    }
    if (it.flags & FLAG_ADDED) {
      ctx.font = italicFont;
      ctx.fillStyle = p.added ?? "#6b6862";
    } else if (it.flags & FLAG_DIVINE) {
      ctx.font = bodyFont;
      ctx.fillStyle = p.divine ?? "#4d3326";
    } else if (it.flags & FLAG_TITLE) {
      ctx.font = bodyFont;
      ctx.fillStyle = p.titleInk ?? "#665c4d";
    } else {
      ctx.font = bodyFont;
      ctx.fillStyle = p.ink ?? "#211f1a";
    }
    ctx.fillText(it.text, x, y);
    // NO mark for a Strong's-tagged word. A faint gold rule under every one of
    // them — and most words carry a Strong's number — amounts to underlining the
    // Bible: visual noise that tells the reader nothing they act on. Whether a
    // word answers when tapped is something you learn once, not something the
    // page needs to keep saying.
    if (it.flags & FLAG_RERENDERED) {
      // The AKJV overlay's mark: DOTTED, at the natural underline depth. Dotted
      // rather than bold or grey because weight and colour are already spoken for
      // (italics mean "supplied by the translator") and because at 6.9% of words
      // a heavier mark would read as a ransom note rather than as text. It also
      // survives a band, which a background tint would not.
      ctx.save();
      ctx.strokeStyle = withAlpha(gold, 0.75);
      ctx.lineWidth = 1;
      ctx.setLineDash([1.5, 2.5]);
      ctx.beginPath();
      const dy = y + fontPx + 2.5;
      ctx.moveTo(x, dy);
      ctx.lineTo(x + it.w, dy);
      ctx.stroke();
      ctx.restore();
    }
  }
}
