import { expect, test, type Page } from "@playwright/test";

// Nothing may hide under the destination bar.
//
// This is a CLASS test, written because the class kept recurring one member at a
// time (2026-07-29: "the new thread box is hidden behind the menu at the bottom of
// the screen — probably a class of bug"). It was: four separate sheets anchored
// themselves at `bottom: 0` on phone widths, and the bar sits there. Only
// PresentHost had ever used `--bottomNavH`, the measured height Shell publishes
// for exactly this.
//
// So rather than asserting the thread picker in particular, this opens EVERY
// surface a reader can put on screen at a phone width and checks the same
// property of each: its box ends at or above the top of the bar. A fifth sheet
// added with `bottom: 0` fails here, which is the point — the individual fixes
// are one-liners, and this is the thing that notices when someone forgets.
//
// Mutation-tested: reverting any one of the four sheets to `bottom: 0` names it in
// the failure ("thread picker: ends at 780px, bar starts at 723px").
//
// It runs at TWO heights, and the short one earns its place. The tall dialogs
// (settings, history) are capped with `calc(Xvh - var(--bottomNavH))`, and on a
// 780px screen removing that cap changes nothing — 8vh + 84vh simply happens to
// land above the bar there, so a one-viewport test would have asserted those caps
// while being blind to them. At 620px the same arithmetic overlaps, so the cap is
// actually held to account.

const VIEWPORTS = [
  { name: "tall phone", width: 390, height: 780 },
  { name: "short phone", width: 390, height: 620 },
];

async function boot(page: Page, vp: { width: number; height: number }): Promise<void> {
  await page.setViewportSize(vp);
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator("nav.bottom-nav")).toBeVisible();
}

/**
 * Every surface a reader can raise, and how to raise it.
 *
 * Driven through session state rather than by clicking, so the table stays about
 * the LAYOUT PROPERTY and not about however each one happens to be reached.
 *
 * Matched on `data-surface`, which exists for this. `.dialog` and `.sheet` are
 * reused by half a dozen components, so a class selector measures whichever is
 * first in the DOM — and it did: this sweep spent a run checking the passage
 * navigator's height while reporting on Settings, and passed a mutation it should
 * have caught. An ambiguous selector in a class guard is worse than no guard.
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
      // One surface at a time: dismissTransient first, so a leftover cannot be
      // the thing measured (and so the sweep also exercises that it clears).
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

  // The box ending above the bar is necessary but not sufficient — what the
  // reader actually lost was the "New thread…" field and its Add button. Measure
  // the control itself, and click it, which is the only proof that nothing is
  // sitting on top of it.
  await page.evaluate(() => ((window as any).__plumbline.threadPickFor = "John 3:16"));
  const field = page.getByPlaceholder("New thread…");
  await expect(field).toBeVisible();

  const navTop = await page.locator("nav.bottom-nav").evaluate((n) => n.getBoundingClientRect().top);
  const box = (await field.boundingBox())!;
  expect(box.y + box.height, "the New thread… field must sit above the bar").toBeLessThanOrEqual(navTop + 1);

  await field.fill("Reachable");
  const add = page.getByRole("button", { name: "Add", exact: true });
  const addBox = (await add.boundingBox())!;
  expect(addBox.y + addBox.height, "and so must its Add button").toBeLessThanOrEqual(navTop + 1);
  await add.click();

  // It really was the button and not the bar behind it.
  await expect(page.locator(".toast")).toContainText("Reachable", { timeout: 15_000 });
});
