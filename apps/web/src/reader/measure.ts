// Text measurement backing the layout engine's injected callback: a shared
// offscreen 2D context + per-font width cache. The engine measures thousands
// of words per chapter layout; the cache makes repeat layouts near-free.

const ctx = document.createElement("canvas").getContext("2d")!;
const caches = new Map<string, Map<string, number>>();

export const READER_FONT_FAMILY = '"EB Garamond", Georgia, serif';

export function readerFont(px: number): string {
  return `${px}px ${READER_FONT_FAMILY}`;
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
