import { expect, test, type Page } from "@playwright/test";

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

test("phone: the four destinations are in the bottom bar, not the menu", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  const nav = page.locator(".bottom-nav");
  await expect(nav).toBeVisible();
  await expect(nav.locator("button")).toHaveCount(4);
  for (const label of ["Read", "Explore", "Present", "Memorize"]) {
    await expect(nav.getByRole("button", { name: label })).toBeVisible();
  }
  // Read is current on arrival, and the icons really are icons.
  await expect(nav.locator("button.on")).toHaveText(/Read/);
  await expect(nav.locator("svg")).toHaveCount(4);

  // The menu keeps utilities ONLY — the whole point of moving them out.
  await page.getByLabel("Menu").click();
  const menu = page.locator(".menu");
  await expect(menu.getByRole("button", { name: "Settings" })).toBeVisible();
  for (const gone of ["Explore", "Present", "Memorize"]) {
    await expect(menu.getByRole("button", { name: gone })).toHaveCount(0);
  }
});

// Present was the one surface that covered the whole chrome, so the four
// destinations vanished the moment you opened it — leaving a ✕ as the only way
// back, on the one screen someone is using in front of other people (feedback
// 2026-07-28). Both of its states are checked: the picker, and a passage actually
// up on screen. They are the same element, and it would be easy to fix the first
// and not notice the second.
test("phone: Present keeps the four destinations, picking and presenting alike", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  const nav = page.locator(".bottom-nav");

  await nav.getByRole("button", { name: "Present" }).click();
  await expect(page.locator(".present")).toBeVisible();
  await expect(nav, "the picker covered the bottom bar").toBeVisible();
  await expect(nav.locator("button.on")).toHaveText(/Present/);

  // ...and with something actually being presented. UNCONDITIONALLY: the stock
  // set seeds the Romans Road thread, so the picker always has one to choose, and
  // an `if` around this half would quietly skip the state that matters on any run
  // where the seeding broke.
  const picks = page.locator(".present .pick");
  await expect(picks.first(), "the stock set should give the picker a thread").toBeVisible();
  await picks.first().click();
  await expect(page.locator(".present.picking")).toHaveCount(0);
  await expect(nav, "presenting a passage covered the bottom bar").toBeVisible();
  await expect(nav.getByRole("button", { name: "Read" })).toBeVisible();

  // GEOMETRY, not just visibility. `toBeVisible` is satisfied by an element that
  // is painted over by something else — the first version of this test passed
  // with Present still spanning the full viewport, because raising the bar's
  // z-index kept it clickable and that was all anything checked. What the fix
  // actually has to guarantee is that Present STOPS ABOVE the bar, so its own
  // controls are not sitting underneath it.
  const pres = await page.locator(".present").boundingBox();
  const bar = await nav.boundingBox();
  expect(pres && bar, "both surfaces should be on screen").toBeTruthy();
  expect(
    Math.round(pres!.y + pres!.height),
    `Present runs to ${Math.round(pres!.y + pres!.height)}px but the bar starts at ` +
      `${Math.round(bar!.y)}px — it is underlapping the bar, so whatever Present ` +
      `draws at its bottom edge is hidden behind the destinations`,
  ).toBeLessThanOrEqual(Math.round(bar!.y));

  // The bar still works from in there — that is the point of keeping it.
  await nav.getByRole("button", { name: "Read" }).click();
  await expect(page.locator(".present")).toHaveCount(0);
});

test("phone: the bottom bar switches destinations and tracks which is current", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  const nav = page.locator(".bottom-nav");

  await nav.getByRole("button", { name: "Explore" }).click();
  await expect(nav.locator("button.on")).toHaveText(/Explore/);

  await nav.getByRole("button", { name: "Memorize" }).click();
  await expect(nav.locator("button.on")).toHaveText(/Memorize/);
  // NOT asserting that Memorize discards the Explore panel. Android's four
  // destinations are exclusive because it shows one screen at a time; the web
  // layers, and on a DESKTOP the study panel is a sidebar — keeping Explore
  // behind a fullscreen Present so it is still there on the way back is the right
  // behaviour there, and `go()` is shared by both widths. What matters, and is
  // asserted, is that the highlighted tab always names the surface actually in
  // front of the reader.

  // Read is the absence of a destination: it clears whatever is layered over
  // the reader rather than opening anything.
  await nav.getByRole("button", { name: "Read" }).click();
  await expect(nav.locator("button.on")).toHaveText(/Read/);
  const state = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { panel: s.panel, memorize: s.memorize, present: s.showPresent };
  });
  expect(state).toEqual({ panel: null, memorize: null, present: false });
  await expect(page.locator(".pane canvas").first()).toBeVisible();
});

test("the bottom bar does not cover the reader, and is absent on a desktop width", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  // The bar sits BELOW the pane, not over it: a nav bar overlapping the last
  // line of scripture is worse than no nav bar.
  const pane = (await page.locator(".panes").boundingBox())!;
  const bar = (await page.locator(".bottom-nav").boundingBox())!;
  expect(bar.y).toBeGreaterThanOrEqual(pane.y + pane.height - 1);
  expect(bar.height).toBeGreaterThanOrEqual(48); // a real thumb target

  await page.setViewportSize({ width: 1280, height: 900 });
  await expect(page.locator(".bottom-nav")).toBeHidden();
  // …because at that width the destinations are first-class in the top bar.
  await expect(page.locator("nav.browse").getByRole("button", { name: "Explore" })).toBeVisible();
});
