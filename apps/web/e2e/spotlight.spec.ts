import { expect, test, type Page } from "@playwright/test";

// The weekly feature tile (shell/spotlight.ts, maintainer 2026-10-29): once a
// week the bookmarks row carries a lightbulb chip naming a feature; its card
// explains the feature and opens it, and Done clears it until the next week.
// A feature the reader already uses is passed over.
//
// Can it fail? Drop the pick and no chip ever appears; stop persisting Done and
// the chip is back after the reload; lose the usage signal and the Notes tile
// shows to a reader who has notes; break the wrap and History is followed by
// nothing.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

const spot = (page: Page) => page.evaluate(() => (window as any).__plumbline.config.spotlight ?? null);

test("the week's feature chip appears on a fresh install, opens its card, and Done clears it for the week", async ({ page }) => {
  await boot(page);

  // A fresh install uses nothing, so the first feature of the cycle is picked.
  const chip = page.locator(".spot-chip");
  await expect(chip).toBeVisible({ timeout: 30_000 });
  await expect(chip).toHaveAttribute("data-spotlight", "wordStudy");
  await expect(chip).toContainText("Word study");
  expect(await spot(page)).toMatchObject({ id: "wordStudy", done: false });

  await chip.click();
  const card = page.locator('[data-surface="spotlight"]');
  await expect(card).toBeVisible();
  await expect(card).toContainText("Word study");
  await expect(card).toContainText("Double-click a word");
  await expect(card).toContainText("Any word in the text.");

  // Escape keeps the chip: the reader may want it later in the week.
  await page.keyboard.press("Escape");
  await expect(card).toHaveCount(0);
  await expect(chip).toBeVisible();

  await chip.click();
  await card.getByRole("button", { name: "Done" }).click();
  await expect(card).toHaveCount(0);
  await expect(chip).toHaveCount(0);
  expect(await spot(page)).toMatchObject({ id: "wordStudy", done: true });

  // Done is in the config, so a reload within the week stays quiet. The config
  // write is debounced (saveConfig, 300 ms); a reload inside that window is a
  // reader closing the tab the instant they pressed Done, not this test's claim.
  await page.waitForTimeout(800);
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  // Give the pick effect every chance to run before claiming it stayed quiet.
  await page.waitForTimeout(1_500);
  await expect(page.locator(".spot-chip")).toHaveCount(0);
});

test("a new week picks the next feature the reader does not use, Open goes there, and the cycle wraps", async ({ page }) => {
  await boot(page);
  await expect(page.locator(".spot-chip")).toBeVisible({ timeout: 30_000 });

  // Last week's tile was Search, and this reader writes notes — so Notes is
  // passed over and Tags is next.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteAdd("John 3:16", "spotlight probe", "2026-08-13T00:00:00Z");
    s.invalidate();
    s.config.spotlight = { week: "2000-01-03", id: "search", done: true };
    s.saveConfig();
  });
  const chip = page.locator(".spot-chip");
  await expect(chip).toHaveAttribute("data-spotlight", "tags", { timeout: 30_000 });
  await expect(chip).toContainText("Tags");

  // Open goes to the feature and counts as dealt with.
  await chip.click();
  await page.locator('[data-surface="spotlight"]').getByRole("button", { name: "Open" }).click();
  await expect(page.getByRole("region", { name: "Tags" })).toBeVisible();
  expect(await spot(page)).toMatchObject({ id: "tags", done: true });

  // After the last feature the cycle starts again from the top.
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.goRead();
    s.config.spotlight = { week: "2000-01-03", id: "history", done: true };
    s.saveConfig();
  });
  await expect(chip).toHaveAttribute("data-spotlight", "wordStudy", { timeout: 30_000 });
});
