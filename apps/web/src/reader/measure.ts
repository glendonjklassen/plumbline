// Text measurement backing the layout engine's injected callback: a shared
// offscreen 2D context + per-font width cache. The engine measures thousands
// of words per chapter layout; the cache makes repeat layouts near-free.

// Works on the main thread AND in the engine worker (TODO #28): the layout
// measure callback runs where the engine runs, over an OffscreenCanvas there.
import { DEFAULT_FONT, FONT_CSS_FAMILY, FONT_FALLBACK, FONT_FILES, FONT_SCALE } from "../engine/fonts.generated";

const ctx = (typeof document !== "undefined"
  ? document.createElement("canvas").getContext("2d")
  : new OffscreenCanvas(2, 2).getContext("2d"))! as CanvasRenderingContext2D;
// Keyed by the FULL font string, which carries the family — so a width measured
// under one face can never be served for another.
const caches = new Map<string, Map<string, number>>();

// Pinned for the reason `reader/paint.ts` pins the same two: this context and
// that one have to agree, and `direction` defaults to `inherit` — which resolves
// against a document this OffscreenCanvas does not have. An advance width does
// not depend on direction today, so this changes no number; it is here so that
// the measuring context and the painting context are configured alike on the
// axis where they could silently drift apart.
ctx.textAlign = "left";
ctx.direction = "ltr";

// The reader's face is a SETTING (config `textFont`), so it is state rather than
// a constant — and this module is loaded in BOTH the engine worker (which
// measures) and the main thread (which paints), each with its own copy. Both
// must be told the same token, or lines wrap where they are not drawn; the
// worker is told in its boot/`setTextFont` message and the main thread by the
// shell, from the one config value.
let fontToken: string = DEFAULT_FONT;
let fontStack: string = fontStackFor(DEFAULT_FONT);
let fontScale: number = FONT_SCALE[DEFAULT_FONT] ?? 1;

/** The CSS family stack for a token — `"Family", fallback`. Exported so the
 *  DOCUMENT (chrome) and the CANVAS (scripture) build the same string from the
 *  same table. */
export function fontStackFor(token: string): string {
  const css = FONT_CSS_FAMILY[token] ?? FONT_CSS_FAMILY[DEFAULT_FONT];
  const fallback = FONT_FALLBACK[token] ?? FONT_FALLBACK[DEFAULT_FONT];
  return `"${css}", ${fallback}`;
}

/** Point this context at a face. Unknown tokens (a config from a later build)
 *  resolve to the default rather than to an unstyled fallback. Returns the token
 *  actually adopted. */
export function setReaderFont(token: string): string {
  fontToken = FONT_FILES[token] ? token : DEFAULT_FONT;
  fontStack = fontStackFor(fontToken);
  fontScale = FONT_SCALE[fontToken] ?? 1;
  return fontToken;
}

/** The token this context is currently measuring/painting with. */
export function readerFontToken(): string {
  return fontToken;
}

/** Whether the current face ships an italic. False → translator-supplied words
 *  are told apart by the palette's `added` tone alone (see `core::font`). */
export function readerFontHasItalic(): boolean {
  return FONT_FILES[fontToken]?.italic !== undefined;
}

/** The CSS family stack for the current face — what a `ctx.font` string or a
 *  `font-family` must name. */
export function readerFontFamily(): string {
  return fontStack;
}

/** The px the current face actually renders at for a requested size — the
 *  face's optical scale (`FONT_SCALE`, mirroring `core::font::Font::scale`)
 *  applied to the reader's setting, so switching faces changes the voice of the
 *  text without changing its apparent size.
 *
 *  THIS IS THE ONE PLACE THE SCALE IS APPLIED, and both threads must go through
 *  it: the engine worker measures with [readerFont]/[fontExtent] and the main
 *  thread paints with fonts built on this number (reader/paint.ts). Apply it on
 *  one side only and the engine measures one size while the shell paints
 *  another — lines wrap where they are not drawn.
 *
 *  Render-time only: `config.bodySize` keeps the number the reader chose, or
 *  their size would drift every time they switch faces. */
export function readerFontPx(px: number): number {
  // Two decimals, so the string in a font-cache key is stable rather than
  // carrying float noise.
  return Math.round(px * fontScale * 100) / 100;
}

export function readerFont(px: number): string {
  return `${readerFontPx(px)}px ${fontStack}`;
}

/** A measure function for `font`, cached per unique string. */
export function measureFor(font: string): (text: string) => number {
  let cache = caches.get(font);
  if (!cache) {
    cache = new Map();
    caches.set(font, cache);
  }
  return (text) => {
    let w = cache.get(text);
    if (w === undefined) {
      if (ctx.font !== font) ctx.font = font;
      w = ctx.measureText(text).width;
      cache.set(text, w);
    }
    return w;
  };
}

/** Font metrics for line-height math (ascent+descent in px). */
export function fontExtent(px: number): number {
  ctx.font = readerFont(px);
  const m = ctx.measureText("Mg");
  return m.fontBoundingBoxAscent + m.fontBoundingBoxDescent;
}
