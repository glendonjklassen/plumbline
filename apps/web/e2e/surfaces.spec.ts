import { expect, test, type Page } from "@playwright/test";

// Nothing may hide under the destination bar. A class test: several sheets anchored themselves at
// `bottom: 0` on phone widths, where the bar sits, instead of using `--bottomNavH`, the measured
// height Shell publishes for exactly this. So rather than asserting one sheet, it opens every
// surface a reader can raise at a phone width and checks the same property of each — its box ends
// at or above the top of the bar. A new sheet added with `bottom: 0` fails here.
//
// Both heights earn their place. The tall dialogs (settings, history) are capped with
// `calc(Xvh - var(--bottomNavH))`, and on a 780px screen removing that cap changes nothing — 8vh +
// 84vh happens to land above the bar there, so a one-viewport test would assert those caps while
// being blind to them. At 620px the same arithmetic overlaps and the cap is held to account.

const VIEWPORTS = [
  { name: "tall phone", width: 390, height: 780 },
  { name: "short phone", width: 390, height: 620 },
];

async function boot(page: Page, vp: { width: number; height: number }): Promise<void> {
  await page.setViewportSize(vp);
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator("nav.bottom-nav")).toBeVisible();
}

/**
 * Every surface a reader can raise, and how to raise it. Driven through session state rather than
 * by clicking, so the table stays about the layout property.
 *
 * Matched on `data-surface`, which exists for this: `.dialog` and `.sheet` are reused by half a
 * dozen components, so a class selector measures whichever is first in the DOM and can report on
 * a different surface than the one it names.
 */
const SURFACES: { name: string; open: string }[] = [
  { name: "thread picker", open: `s.threadPickFor = "John 3:16"` },
  { name: "tag picker", open: `s.tagPickFor = "John 3:16"` },
  { name: "study panel", open: `s.panel = { kind: "guide" }` },
  { name: "settings", open: `s.showSettings = true` },
  { name: "history", open: `s.showHistory = true` },
  { name: "mark read", open: `s.markReadFor = { book: "Gen", chapter: 1 }` },
  { name: "passage picker", open: `s.memorizePassageFrom = "John 3:16"` },
];

for (const vp of VIEWPORTS) {
  test(`no surface hides under the destination bar (${vp.name})`, async ({ page }) => {
    await boot(page, vp);

    const navTop = await page.locator("nav.bottom-nav").evaluate((n) => n.getBoundingClientRect().top);
    expect(navTop, "the bar should sit at the bottom of the screen").toBeGreaterThan(vp.height * 0.7);

    const offenders: string[] = [];
    for (const s of SURFACES) {
      // One surface at a time: dismissTransient first, so a leftover cannot be the thing measured.
      await page.evaluate(() => (window as any).__plumbline.dismissTransient());
      await page.evaluate(`(() => { const s = window.__plumbline; ${s.open}; })()`);

      const el = page.locator(`[data-surface="${s.name}"]`);
      await expect(el, `${s.name} should open`).toBeVisible({ timeout: 15_000 });
      const bottom = await el.evaluate((n) => n.getBoundingClientRect().bottom);
      // 1px of slack for sub-pixel layout; anything more is a real overlap.
      if (bottom > navTop + 1) {
        offenders.push(`${s.name}: ends at ${Math.round(bottom)}px, bar starts at ${Math.round(navTop)}px`);
      }
    }
    await page.evaluate(() => (window as any).__plumbline.dismissTransient());

    expect(offenders, `these surfaces run under the destination bar:\n  ${offenders.join("\n  ")}`).toEqual([]);
  });
}

test("a bottom sheet's own last control is reachable, not just its box", async ({ page }) => {
  await boot(page, VIEWPORTS[0]);

  // The box ending above the bar is necessary but not sufficient: what the reader lost was the
  // "New thread…" field and its Add button. Clicking the control is the only proof that nothing
  // is sitting on top of it.
  await page.evaluate(() => ((window as any).__plumbline.threadPickFor = "John 3:16"));
  const field = page.getByPlaceholder("New thread…");
  await expect(field).toBeVisible();

  const navTop = await page.locator("nav.bottom-nav").evaluate((n) => n.getBoundingClientRect().top);
  const box = (await field.boundingBox())!;
  expect(box.y + box.height, "the New thread… field must sit above the bar").toBeLessThanOrEqual(navTop + 1);

  await field.fill("Reachable");
  const add = page.getByRole("button", { name: "＋", exact: true });
  const addBox = (await add.boundingBox())!;
  expect(addBox.y + addBox.height, "and so must its Add button").toBeLessThanOrEqual(navTop + 1);
  await add.click();

  // It really was the button and not the bar behind it.
  await expect(page.locator(".toast")).toContainText("Reachable", { timeout: 15_000 });
});
