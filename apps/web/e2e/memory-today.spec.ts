import { expect, test, type Page } from "@playwright/test";

// Memorization scheduled with the reading plans (maintainer, 2026-10-24): the
// plans screen carries a Memorize card with today's capped queue and the
// per-day cap, and the bookmark row a chip for the same queue; a review
// launched from either walks exactly that slice, and grading it retires both
// for the day. Dies if the card or chip stop reading `memoryToday`, if the cap
// stops being honoured, or if a grade stops counting against it.
//
// Can it fail? Drop the cap from the core's queue and the chip reads "3 to
// review" against the asserted 2; stop counting today's grades and the card
// keeps offering a verse after two are graded.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

test("today's memorization rides the plans surface under a daily cap", async ({ page }) => {
  await boot(page);

  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const now = new Date().toISOString();
    for (const ref of ["John 3:16", "Ps 23:1", "Gen 1:1"]) await s.author("memoryAdd", ref, now);
    s.setMemorizePerDay(2);
  });

  // The bookmark row: two of the three, because the cap says two.
  const chip = page.locator(".plan-chip-row [data-memory]");
  await expect(chip).toBeVisible({ timeout: 10_000 });
  await expect(chip).toHaveAttribute("data-memory", "2");
  await expect(chip).toContainText("2 to review");

  // The plans screen's card says the same, with the cap's bookkeeping.
  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  const card = page.locator('[data-surface="memory-today"]');
  await expect(card).toBeVisible();
  await expect(card).toContainText("2 verses to review today");
  await expect(card).toContainText("0 of 2 reviewed today");
  await expect(card.locator('input[type="number"]')).toHaveValue("2");

  // Review from the card: exactly the two scheduled cards, then the day is done.
  await card.getByRole("button", { name: /to review today/ }).click();
  for (let i = 0; i < 2; i++) {
    await page.getByRole("button", { name: "Good", exact: true }).click();
  }
  await expect(page.getByText("Done — every card reviewed.")).toBeVisible();

  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  await expect(card).toContainText("Done for today.");
  await expect(card).toContainText("2 of 2 reviewed today");

  // The chip retires with the day's cap spent, though a card is still due.
  await page.evaluate(() => (((window as any).__plumbline as any).screen = "read"));
  await expect(page.locator(".plan-chip-row [data-memory]")).toHaveCount(0);

  // No limit: the remaining card is offered again.
  await page.evaluate(() => (window as any).__plumbline.setMemorizePerDay(0));
  await expect(page.locator(".plan-chip-row [data-memory]")).toHaveAttribute("data-memory", "1");
});
