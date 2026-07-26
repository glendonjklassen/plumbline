// The reader's chapter painter — the web twin of the GTK/WinUI canvas paint
// (manifest §Reader core): verse numbers bold gold; FLAG_ADDED italic gray;
// FLAG_DIVINE / FLAG_TITLE inks; Strong's underline; highlight bands, washes,
// word-precise runs, pinned spans, and the weave/note gutter marks.

import { READER_FONT_FAMILY } from "./measure";

export const MARGIN = 28; // top/bottom text margin (manifest constant)

export const FLAG_ADDED = 1;
export const FLAG_DIVINE = 2;
export const FLAG_TITLE = 4;

export interface LayoutItem {
  x: number;
  y: number;
  w: number;
  h: number;
  text: string;
  kind: "verseNumber" | "word";
  verse: string | null;
  verseDisplay: string | null;
  tokenIndex: number | null;
  verseNumber: number | null;
  flags: number;
  strongs: string[];
}

export interface WordRun {
  verse: number;
  lo: number;
  hi: number;
  color: string;
}

export interface PaintOverlays {
  /** Verse number to band (search/goto target) — gold wash over its lines. */
  bandVerse?: number | null;
  /** Verses with search hits — banded like the target (manifest Tier-0 #8). */
  hitVerses?: Set<number>;
  /** Whole-verse highlight washes: verse number → tone hex. */
  washes?: Map<number, string>;
  /** Word-precise highlight runs (drag ranges decomposed per verse). */
  runs?: WordRun[];
  /** Live drag preview (per-verse runs), painted in the default tone. */
  dragPreview?: WordRun[] | null;
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

/** Per-verse vertical extents (layout coords), for bands and washes. */
export function verseExtents(items: LayoutItem[]): Map<number, { top: number; bottom: number; firstY: number }> {
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
  return out;
}

function withAlpha(hex: string, alpha: number): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha})`;
}

export function paintChapter(
  ctx: CanvasRenderingContext2D,
  items: LayoutItem[],
  o: PaintOpts,
  ov: PaintOverlays,
): void {
  const { palette: p, fontPx, marginX, columnWidth, scrollY, viewportW, viewportH } = o;
  ctx.fillStyle = p.paper ?? "#fcf9f4";
  ctx.fillRect(0, 0, viewportW, viewportH);

  const yOf = (layoutY: number) => MARGIN + layoutY - scrollY;
  const visible = (top: number, bottom: number) => yOf(bottom) >= 0 && yOf(top) <= viewportH;
  const extents = verseExtents(items);

  // ── verse-band washes: tone washes under, then hit/target bands over ──
  const bandRect = (e: { top: number; bottom: number }) =>
    [marginX - 6, yOf(e.top), columnWidth + 12, e.bottom - e.top] as const;

  if (ov.washes)
    for (const [v, tone] of ov.washes) {
      const e = extents.get(v);
      if (e && visible(e.top, e.bottom)) {
        ctx.fillStyle = withAlpha(tone, 0.45);
        ctx.fillRect(...bandRect(e));
      }
    }
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

  // ── word-precise runs (highlight ranges, drag preview) ──
  const paintRun = (run: WordRun, color: string, alpha: number) => {
    for (const it of items) {
      if (it.kind !== "word" || itemVerse(it) !== run.verse) continue;
      const t = it.tokenIndex ?? -1;
      if (t < run.lo || t > run.hi) continue;
      if (!visible(it.y, it.y + it.h)) continue;
      ctx.fillStyle = withAlpha(color, alpha);
      ctx.fillRect(marginX + it.x - 1, yOf(it.y), it.w + 2, it.h);
    }
  };
  if (ov.runs) for (const r of ov.runs) paintRun(r, r.color, 0.45);
  if (ov.dragPreview) for (const r of ov.dragPreview) paintRun(r, r.color, 0.35);

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
  const bodyFont = `${fontPx}px ${READER_FONT_FAMILY}`;
  const italicFont = `italic ${fontPx}px ${READER_FONT_FAMILY}`;
  const boldFont = `bold ${fontPx}px ${READER_FONT_FAMILY}`;
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
    if (it.strongs.length) {
      // Strong's-tagged: gold underline at baseline+2.5 (α0.30, width 1).
      ctx.strokeStyle = withAlpha(gold, 0.3);
      ctx.lineWidth = 1;
      ctx.beginPath();
      const uy = y + fontPx + 2.5;
      ctx.moveTo(x, uy);
      ctx.lineTo(x + it.w, uy);
      ctx.stroke();
    }
  }
}
