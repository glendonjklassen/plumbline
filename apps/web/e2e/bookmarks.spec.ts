import { expect, test, type Page } from "@playwright/test";

// The bookmarks strip (maintainer ask, 2026-08-24): the row above the canon
// strip — grown out of the plan chip — carries a swipeable tile per stored
// bookmark: the running plan, then every seating position in `config.slots`
// (Last opened, Sunday morning, Sunday evening, Wednesday evening), each with
// an icon naming its kind. A tap toasts WHICH bookmark it was and where it is
// going, then navigates — four tiles all reading "John 3" are otherwise
// indistinguishable mid-swipe.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1100, height: 800 });
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

test("a stored seating rides the strip; a tap says which bookmark and goes there", async ({ page }) => {
  await boot(page);
  // Plant a Sunday-morning seating other than where the reader is.
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.slots = { ...(s.config.slots ?? {}), "sunday-morning": { book: "Ps", chapter: 23, verse: 4 } };
  });

  const tile = page.locator('.bm-tile[data-slot="sunday-morning"]');
  await expect(tile).toContainText("Sunday morning");
  await expect(tile).toContainText("Psalms 23:4");
  // The icon names the tile's kind for the eye the label serves for the reader.
  await expect(tile.locator("svg")).toHaveCount(1);

  await tile.click();
  // The toast names the BOOKMARK as well as the destination.
  await expect(page.locator(".toast")).toContainText("Sunday morning — going to Psalms 23:4");
  await expect(page.locator(".subtitle")).toHaveText("Psalms 23", { timeout: 30_000 });
});

test("the everyday seating shows as Last opened", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.slots = { other: { book: "Rom", chapter: 8 } };
  });
  const tile = page.locator('.bm-tile[data-slot="other"]');
  await expect(tile).toContainText("Last opened");
  await expect(tile).toContainText("Romans 8");
});
