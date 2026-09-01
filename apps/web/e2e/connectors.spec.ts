// Weave connectors are drawn on one overlay canvas spanning the whole pane row, so every endpoint
// is offset by the pane chrome standing above the text — and that offset was a hardcoded 33px.
// When the nav strip grew, the overlay never heard, and every connector ended up some 25px clear
// of its verse.
//
// So this asserts geometry at two chrome heights: the connector's endpoint dot lines up in y with
// the pane's own weave gutter dot for that verse. Those are two independent paints of one verse's
// position, and no constant can satisfy both heights at once. The second height is injected CSS
// rather than a setting, because nothing in the web chrome is font-scaled today.
import { expect, test, type Page } from "@playwright/test";

/** Endpoint-to-verse slack we allow (px): a pixel or two of centroid noise, no
 *  more. The bug this guards was 25px at the shipped chrome height. */
const TOL = 2;

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

interface Woven {
  a: string;
  b: string;
  aVerse: number;
  bVerse: number;
}

/** Put the two chapters of one stock weave link side by side. The link is chosen from the engine's
 *  own pairs rather than named here, so the test does not rot when the stock set changes: it wants
 *  a pair whose two chapters each hold exactly one woven verse near the top, so there is one gutter
 *  dot and one connector per pane and no scrolling to place them. */
async function openWovenPair(page: Page): Promise<Woven> {
  const pick = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const pairs: any[] = ((await s.fetchQ("linkPairs"))?.pairs ?? []).filter((p: any) => p.resolved);
    const wovenVersesIn = (book: string, chapter: number) => {
      const set = new Set<number>();
      for (const p of pairs) {
        if (p.aBook === book && p.aChapter === chapter) set.add(p.aVerse);
        if (p.bBook === book && p.bChapter === chapter) set.add(p.bVerse);
      }
      return set;
    };
    const usable = pairs
      .filter(
        (p) =>
          !(p.aBook === p.bBook && p.aChapter === p.bChapter) &&
          p.aVerse <= 8 &&
          p.bVerse <= 8 &&
          wovenVersesIn(p.aBook, p.aChapter).size === 1 &&
          wovenVersesIn(p.bBook, p.bChapter).size === 1,
      )
      .sort((x, y) => (x.a as string).localeCompare(y.a));
    const p = usable[0];
    if (!p) return null;
    s.navigate(0, p.aBook, p.aChapter);
    if (s.panes.length < 2) s.addPane(0);
    s.navigate(1, p.bBook, p.bChapter);
    return { a: p.a, b: p.b, aVerse: p.aVerse, bVerse: p.bVerse };
  });
  expect(
    pick,
    "the stock weaves should hold one link whose two chapters each carry a single woven verse",
  ).not.toBeNull();
  const woven = pick as Woven;
  await page.waitForFunction(
    (v: { a: number; b: number }) => {
      const s = (window as any).__plumbline;
      return !!s.paneVerseGeom[0]?.get(v.a) && !!s.paneVerseGeom[1]?.get(v.b);
    },
    { a: woven.aVerse, b: woven.bVerse },
    { timeout: 60_000 },
  );
  return woven;
}

interface Aligned {
  /** Pane chrome above the text, per pane — the offset the overlay used to hardcode. */
  strip: number[];
  /** Viewport y of each pane's weave gutter dot: where the verse actually is. */
  gutter: (number | null)[];
  /** Where the connector OUGHT to meet that verse, derived from the gutter dot. */
  want: (number | null)[];
  /** Viewport y of each drawn connector endpoint dot. */
  endpoint: (number | null)[];
  paneTop: number;
  paneBottom: number;
}

/** Read both paints back out of the pixels, finding the dots by what they are rather than by any
 *  of the overlay's own numbers. */
async function readAlignment(page: Page, woven: Woven): Promise<Aligned> {
  // A chrome change reaches the overlay through a ResizeObserver and is painted in a rAF after
  // that, so the frames have to be given away before the pixels mean anything.
  await page.waitForTimeout(500);
  await page.evaluate(
    () =>
      new Promise((r) =>
        requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(r))),
      ),
  );
  return await page.evaluate((woven: Woven) => {
    const s = (window as any).__plumbline;
    const row = document.querySelector<HTMLElement>(".panes")!;
    const rowTop = row.getBoundingClientRect().top;
    const panes = [...document.querySelectorAll<HTMLElement>(".pane")];

    /** Weighted centre of the pixels a filter accepts, in CSS px. */
    const centre = (
      c: HTMLCanvasElement,
      keep: (r: number, g: number, b: number, a: number, x: number) => boolean,
    ): { x: number; y: number } | null => {
      const px = c.getContext("2d")!.getImageData(0, 0, c.width, c.height).data;
      const dpr = c.width / c.getBoundingClientRect().width;
      let n = 0;
      let sx = 0;
      let sy = 0;
      for (let y = 0; y < c.height; y++)
        for (let x = 0; x < c.width; x++) {
          const i = (y * c.width + x) * 4;
          if (!keep(px[i], px[i + 1], px[i + 2], px[i + 3], x)) continue;
          n++;
          sx += x;
          sy += y;
        }
      return n ? { x: sx / n / dpr, y: sy / n / dpr } : null;
    };

    // The endpoint dots are the only thing drawn at α0.7 (the Bézier between them is α0.35), so a
    // mid threshold picks out the ends alone — one per side of the row.
    const overlay = document.querySelector<HTMLCanvasElement>(".panes .overlay canvas")!;
    const half = overlay.width / 2;
    const endpoint = [0, 1].map((i) => {
      const inPane = (x: number) => (i === 0 ? x < half : x >= half);
      const hit = centre(overlay, (_r, _g, _b, a, x) => a > 120 && inPane(x));
      return hit ? overlay.getBoundingClientRect().top + hit.y : null;
    });

    // The pane's own witness: the gold gutter dot beside a woven verse (α0.75 gold over paper). It
    // is the leftmost gold thing on the page — verse numbers begin some 7px to its right — so the
    // leftmost gold column and its neighbours are the dot and nothing else.
    const gutter = panes.map((pane) => {
      const c = pane.querySelector<HTMLCanvasElement>("canvas")!;
      const goldish = (r: number, g: number, b: number) => r > g && g > b && r - b > 40;
      let minX = c.width;
      const px = c.getContext("2d")!.getImageData(0, 0, c.width, c.height).data;
      for (let y = 0; y < c.height; y++)
        for (let x = 0; x < minX; x++) {
          const i = (y * c.width + x) * 4;
          if (goldish(px[i], px[i + 1], px[i + 2])) minX = x;
        }
      if (minX >= c.width) return null;
      const dpr = c.width / c.getBoundingClientRect().width;
      const hit = centre(c, (r, g, b, _a, x) => goldish(r, g, b) && x <= minX + 6 * dpr);
      return hit ? c.getBoundingClientRect().top + hit.y : null;
    });

    // The two dots sit a known distance apart: the pane puts its gutter dot 0.55em below the top
    // of the verse's first line (reader/paint.ts) and a connector meets the centre of that line.
    // Both come off the same layout entry, so the gap is arithmetic rather than slack, which is
    // what lets the tolerance be a pixel or two instead of half a line.
    const fontPx = Number(s.config.bodySize ?? 18);
    const want = [woven.aVerse, woven.bVerse].map((verse, i) => {
      const h = s.paneVerseGeom[i]?.get(verse)?.h;
      return gutter[i] === null || h === undefined ? null : gutter[i]! + h / 2 - 0.55 * fontPx;
    });

    return {
      strip: panes.map(
        (pane) => pane.querySelector<HTMLElement>(".scroll")!.getBoundingClientRect().top - rowTop,
      ),
      gutter,
      want,
      endpoint,
      paneTop: rowTop,
      paneBottom: rowTop + row.getBoundingClientRect().height,
    };
  }, woven);
}

function expectConnectorsMeetTheirVerses(m: Aligned, woven: Woven, where: string): void {
  for (const i of [0, 1]) {
    const ref = i === 0 ? woven.a : woven.b;
    expect(m.endpoint[i], `${where}: no connector endpoint drawn on pane ${i}'s edge`).not.toBeNull();
    expect(m.gutter[i], `${where}: pane ${i} painted no weave gutter dot for ${ref}`).not.toBeNull();
    // Both ends must be well inside the pane, or a clamped endpoint could pass for an aligned one.
    expect(m.gutter[i]!, `${where}: ${ref} is not clear of the pane's edges`).toBeGreaterThan(
      m.paneTop + m.strip[i] + 20,
    );
    expect(m.gutter[i]!, `${where}: ${ref} is not clear of the pane's edges`).toBeLessThan(
      m.paneBottom - 20,
    );
    expect(m.want[i], `${where}: no laid-out geometry for ${ref}`).not.toBeNull();
    const off = Math.abs(m.endpoint[i]! - m.want[i]!);
    expect(
      off,
      `${where}: the connector meets pane ${i} at y=${m.endpoint[i]!.toFixed(1)}, but ${ref} is on ` +
        `screen at y=${m.want[i]!.toFixed(1)} (its gutter dot is at ${m.gutter[i]!.toFixed(1)}) — ` +
        `${off.toFixed(1)}px out, with ${m.strip[i].toFixed(1)}px of pane chrome above the text. ` +
        `The overlay is not measuring the strip it draws below.`,
    ).toBeLessThanOrEqual(TOL);
  }
}

test("a weave connector meets its verse, at either chrome height", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 950 });
  await boot(page);
  const woven = await openWovenPair(page);

  const shipped = await readAlignment(page, woven);
  expectConnectorsMeetTheirVerses(shipped, woven, "the shipped chrome");

  // Resize the chrome underneath it, which is the change that caused the bug.
  await page.addStyleTag({ content: ".pane .nav { padding-top: 34px !important; }" });
  const taller = await readAlignment(page, woven);
  expect(
    taller.strip[0] - shipped.strip[0],
    `the injected chrome should really change the strip: it went ${shipped.strip[0]} → ${taller.strip[0]}`,
  ).toBeGreaterThan(20);
  expectConnectorsMeetTheirVerses(taller, woven, "a taller chrome");
});
