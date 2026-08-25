import { expect, test, type Page } from "@playwright/test";

// The bookmarks row (maintainer ask, 2026-08-24; icon-only 2026-08-25): the row
// above the canon strip — grown out of the plan chip — carries a round ICON
// chip per stored bookmark: the running plan, then every seating position in
// `config.slots` (Last opened, Sunday morning, Sunday evening, Wednesday
// evening). NO TEXT on the chips: the words ride aria-label/title, and a tap
// toasts WHICH bookmark it was before navigating — with no words on the face,
// the toast is the confirmation. Several chips show at once, so it is plain
// there are more than one and that the row scrolls when they overflow.

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

test("a stored seating is an icon chip; a tap says which bookmark and goes there", async ({ page }) => {
  await boot(page);
  // Plant a Sunday-morning seating other than where the reader is.
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.slots = { ...(s.config.slots ?? {}), "sunday-morning": { book: "Ps", chapter: 23, verse: 4 } };
  });

  const tile = page.locator('.bm-tile[data-slot="sunday-morning"]');
  // The words are the accessible name, not the face.
  await expect(tile).toHaveAttribute("aria-label", "Sunday morning · Psalms 23:4");
  await expect(tile.locator("svg")).toHaveCount(1);
  expect((await tile.textContent())?.trim(), "NO TEXT on the chip — the icon is the whole face").toBe("");

  await tile.click();
  // The toast names the BOOKMARK, plainly — no "going to…" sentence; the
  // destination shows itself when the pane lands there.
  await expect(page.locator(".toast")).toHaveText("Sunday morning bookmark");
  await expect(page.locator(".subtitle")).toHaveText("Psalms 23", { timeout: 30_000 });
});

test("the everyday seating shows as Last opened", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.slots = { other: { book: "Rom", chapter: 8 } };
  });
  await expect(page.locator('.bm-tile[data-slot="other"]')).toHaveAttribute("aria-label", "Last opened · Romans 8");
});

test("several chips are visible at once, not one page at a time", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 740 }); // a phone, where one-at-a-time hid the rest
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.slots = {
      other: { book: "Rom", chapter: 8 },
      "sunday-morning": { book: "Ps", chapter: 23 },
      "sunday-evening": { book: "John", chapter: 17 },
      "wednesday-evening": { book: "Acts", chapter: 2 },
    };
  });
  const chips = page.locator(".bm-tile");
  await expect(chips).toHaveCount(4);
  for (let i = 0; i < 4; i++) await expect(chips.nth(i)).toBeInViewport();
});
