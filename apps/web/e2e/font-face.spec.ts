import { expect, test, type Page } from "@playwright/test";

// Switching the scripture face changes the VOICE of the text, not its layout's
// truthfulness: the engine worker measures a chapter and the main thread paints
// it, and both must be working at the same px in the same face. Two ways that
// has actually broken, and both are pinned here:
//
//   * the worker's turn cache was keyed by px but not by FACE, so a face switch
//     re-served geometry measured under the old face — Fira Code painted into
//     rects measured for Garamond, i.e. every word overlapping its neighbour;
//   * the per-face optical scale (FONT_SCALE, mirroring core::font::Font::scale)
//     is applied in reader/measure.ts's readerFontPx, which BOTH threads build
//     their fonts from. Applied on one side only, the engine measures one size
//     while the shell paints another — lines wrap where they are not drawn.
//
// The agreement check is direct: take a word the worker measured (its display-
// list rect, via __plumblinePaint.items) and measure the same text on THIS
// thread with the exact font string the last frame painted
// (__plumblinePaint.bodyFont). The two came from different threads and
// different canvases; if any stage disagrees about px or family, the widths
// split by the whole scale factor.

const DESKTOP = { width: 1100, height: 800 };

async function boot(page: Page, vp = DESKTOP): Promise<void> {
  await page.setViewportSize(vp);
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
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

/** The last painted frame, read off the production paint probe. Null until a
 *  frame has been painted (or after the display list was collected). */
async function snap(page: Page): Promise<Snap | null> {
  return await page.evaluate(() => {
    const probe = (window as any).__plumblinePaint;
    const items = probe?.items?.deref?.() ?? null;
    if (!probe?.bodyFont || !items?.length) return null;
    // A real word, long enough that a width disagreement is comfortably larger
    // than sub-pixel noise.
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

/** Measured and painted widths agree within sub-pixel noise. The failure this
 *  guards is never subtle: a px or family disagreement splits them by the whole
 *  optical-scale factor (12%+), not by a rounding error. */
function agreement(s: Snap): number {
  return Math.abs(s.paintedW - s.sampleW);
}
const AGREE_TOL = 1.5;

/** Fira Code's optical scale (`FONT_SCALE` in engine/fonts.generated.ts): a
 *  monospace at the same nominal size reads larger, so it is painted smaller. */
const FIRA_SCALE = 0.88;

/** The reader's text size as the app itself has it. Read rather than written
 *  down: these tests once hardcoded the shipped default (18, then 20), so
 *  changing it failed a suite that was measuring the FACE, not the size. */
async function defaultPx(page: Page): Promise<number> {
  return await page.evaluate(() => Number((window as any).__plumbline.config.bodySize));
}

// Mutations, each of which must fail this test:
//   * apply the multiplier in paint.ts ONLY — `readerFontPx(o.fontPx)` there but
//     `${px}px` reverted to the raw setting in measure.ts's readerFont →
//     'measured and painted widths disagree … Expected: < 1.5, Received: ~7'
//     (the worker measured 18px Fira Code, the frame painted 15.84px).
//   * drop `readerFontToken()` from the turn-cache key in engine.worker.ts →
//     'the chapter was not re-laid-out for the new face' — the switch re-serves
//     Garamond's geometry, so lastY never moves and the widths split too.
// Rebuild before believing either run: the engine path is `pack:wasm` +
// `npm run build`, or Playwright tests the last bundle you packed.
test("switching the scripture face re-lays the chapter, at the face's optical size", async ({ page }) => {
  await boot(page);

  // The default face at the default size: the optical scale of the shipped
  // default is exactly 1.0, so the slider's size IS the painted size — nothing
  // moves for a reader who never opens the picker.
  await expect.poll(() => snap(page), { timeout: 20_000 }).not.toBeNull();
  const size = await defaultPx(page);
  const before = (await snap(page))!;
  expect(before.px).toBe(size);
  expect(agreement(before), "measured and painted widths disagree at boot").toBeLessThan(AGREE_TOL);

  // Garamond → Fira Code: the pair that made the stale-cache bug vivid, since
  // a monospace's advances are nothing like a garalde's.
  await page.evaluate(() => (window as any).__plumbline.setTextFont("fira-code"));
  await expect
    .poll(async () => (await snap(page))?.bodyFont ?? "", { timeout: 20_000 })
    .toContain("Fira Code");
  // The relayout lands as its own paint; wait for geometry from the new face,
  // not just new glyphs over old rects.
  await expect
    .poll(async () => (await snap(page))?.lastY, {
      message: "the chapter was not re-laid-out for the new face",
      timeout: 20_000,
    })
    .not.toBe(before.lastY);

  // POLLED, not read once. The probe reports the LAST paint, and a frame can
  // land between the lastY poll above and a one-shot read here that pairs the
  // new face with a list the new face has not re-laid yet — "new glyphs over
  // old rects", transiently, while relayouts queue behind the background
  // Bible downloads on a loaded machine (CI caught exactly this at 44 px,
  // 2026-08-29). The poll cannot mask the defects this test exists for: a
  // scale applied on one thread only, or a stale turn-cache serving the old
  // face's geometry, disagree PERMANENTLY — the poll times out red on both
  // mutations above, same as the one-shot read did.
  await expect
    .poll(async () => agreement((await snap(page))!), {
      message: "measured and painted widths disagree after the switch",
      timeout: 20_000,
    })
    .toBeLessThan(AGREE_TOL);

  const after = (await snap(page))!;
  // size × 0.88 (FONT_SCALE["fira-code"]) — the painted px carries the optical
  // scale; config.bodySize still says what the reader set.
  expect(after.px).toBeCloseTo(size * FIRA_SCALE, 2);
  expect(
    await defaultPx(page),
    "the optical scale must never be written back into the stored size",
  ).toBe(size);
});

// The face is a config setting, and settings survive a reload. (It once looked
// like it didn't — the stale-cache overlap above made a saved switch paint as
// garbage, which reads as "didn't take".)
test("the chosen face survives a reload, still at its optical size", async ({ page }) => {
  await boot(page);
  const size = await defaultPx(page);
  await page.evaluate(() => (window as any).__plumbline.setTextFont("fira-code"));
  await expect
    .poll(async () => (await snap(page))?.bodyFont ?? "", { timeout: 20_000 })
    .toContain("Fira Code");
  // Make sure the choice really reached the disk before reloading — the worker
  // debounces its persist, and the RPC is ordered so the flush carries the
  // config with it (same discipline as routing.spec.ts).
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
