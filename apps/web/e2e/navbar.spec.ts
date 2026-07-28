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
