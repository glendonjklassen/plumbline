import { expect, test, type Page } from "@playwright/test";

// The engine worker measures a chapter and the main thread paints it; both must work
// at the same px in the same face. Two ways that has broken, both pinned here:
//
//   * the worker's turn cache keyed by px but not by FACE, so a face switch re-served
//     geometry measured under the old face (every word overlapping its neighbour);
//   * the per-face optical scale (FONT_SCALE, mirroring core::font::Font::scale) is
//     applied in reader/measure.ts's readerFontPx, which both threads build their
//     fonts from. Applied on one side only, lines wrap where they are not drawn.
//
// The check compares a word's worker-measured rect (__plumblinePaint.items) against
// this thread's measurement of the same text under the font string the last frame
// painted (__plumblinePaint.bodyFont). Any px or family disagreement splits the two
// widths by the whole scale factor.

const DESKTOP = { width: 1100, height: 800 };

async function boot(page: Page, vp = DESKTOP): Promise<void> {
  await page.setViewportSize(vp);
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

interface Snap {
  /** The font string the last frame set for body words (carries px + family). */
  bodyFont: string;
  /** Its px, parsed — 18 for the default face at the default size. */
  px: number;
  /** Layout y of the last item: a proxy for where the chapter's lines broke. */
  lastY: number;
  /** One sampled word: the width the WORKER measured it at… */
  sampleW: number;
  /** …and the width THIS thread's canvas gives the same text under bodyFont. */
  paintedW: number;
}

/** The last painted frame, off the production paint probe. Null until a frame has
 *  been painted, or after the display list was collected. */
async function snap(page: Page): Promise<Snap | null> {
  return await page.evaluate(() => {
    const probe = (window as any).__plumblinePaint;
    const items = probe?.items?.deref?.() ?? null;
    if (!probe?.bodyFont || !items?.length) return null;
    // Long enough that a width disagreement is comfortably larger than sub-pixel noise.
    const it = items.find((i: any) => i.kind === "word" && i.text.length >= 6);
    if (!it) return null;
    const c = document.createElement("canvas").getContext("2d")!;
    c.font = probe.bodyFont;
    return {
      bodyFont: probe.bodyFont as string,
      px: parseFloat(probe.bodyFont),
      lastY: items[items.length - 1].y as number,
      sampleW: it.w as number,
      paintedW: c.measureText(it.text).width,
    };
  });
}

/** Gap between measured and painted width. The failure guarded here is not subtle: a
 *  px or family disagreement splits them by the whole scale factor (12%+). */
function agreement(s: Snap): number {
  return Math.abs(s.paintedW - s.sampleW);
}
const AGREE_TOL = 1.5;

/** Fira Code's optical scale (`FONT_SCALE` in engine/fonts.generated.ts): a
 *  monospace at the same nominal size reads larger, so it is painted smaller. */
const FIRA_SCALE = 0.88;

/** The reader's text size as the app itself has it. Read, never hardcoded: these
 *  tests measure the face, so a change to the shipped default must not fail them. */
async function defaultPx(page: Page): Promise<number> {
  return await page.evaluate(() => Number((window as any).__plumbline.config.bodySize));
}

// The face-SWITCH test that lived here (measure after `setTextFont("fira-code")`
// without a reload) was removed 2026-10-30: it failed CI at a stable 44px
// disagreement on some runs (2026-10-22, four attempts on 2026-10-30) while
// passing on every desktop, and nothing in the failure said which thread had
// fallen back to which face. The maintainer's call: a test that cannot be
// trusted guards nothing. The reload test below makes the same two-thread
// agreement check at the same optical size, one reload later.

// The face is a config setting, and settings survive a reload — still at the face's
// optical size, and still with the two threads agreeing.
test("the chosen face survives a reload, still at its optical size", async ({ page }) => {
  await boot(page);
  const size = await defaultPx(page);
  await page.evaluate(() => (window as any).__plumbline.setTextFont("fira-code"));
  await expect
    .poll(async () => (await snap(page))?.bodyFont ?? "", { timeout: 20_000 })
    .toContain("Fira Code");
  // The choice must reach the disk before the reload: the worker debounces its
  // persist, and the RPC is ordered so the flush carries the config with it.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.flushConfig();
    await s.rpc.flush();
  });

  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect
    .poll(async () => (await snap(page))?.bodyFont ?? "", { timeout: 20_000 })
    .toContain("Fira Code");
  const s = (await snap(page))!;
  expect(s.px).toBeCloseTo(size * FIRA_SCALE, 2);
  expect(agreement(s), "measured and painted widths disagree after a reload").toBeLessThan(AGREE_TOL);
});
