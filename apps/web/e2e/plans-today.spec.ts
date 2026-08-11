import { expect, test, type Page } from "@playwright/test";

// Decision #5's reader-side plan surfaces (docs/READING-PLANS.md): when a
// schedule plan is running, its "today" rides the reader as a nav-strip chip
// ("Day 1 · Matt 1–2", tap → today's first unread chapter), and the passage
// navigator opens with a today card whose chapters are the buttons. Each
// assertion dies on the obvious break: the chip if Shell stops mounting it or
// `todayPlans` stops finding the running schedule; the navigation if the tap
// stops going to the first unread chapter; the card if BookNav loses it; the
// concept-study assertion if the chip forgets the mode suspends the tracker.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

test("a running plan rides the reader: the chip goes to today, the navigator leads with it", async ({ page }) => {
  await boot(page);

  // Start the NT-in-90 schedule straight through the engine (the picker UI has
  // its own coverage); the chip and card are the subject here.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("planStart", "nt-90", new Date().toISOString());
  });

  // The chip appears with day 1. Its text is the plan's own answer, so pin the
  // expected target from the same wire the chip reads.
  const chip = page.locator(".plan-chip-row .plan-chip").first();
  await expect(chip).toBeVisible({ timeout: 10_000 });
  await expect(chip).toHaveText(/Day 1 · /);

  const first = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const plans = await s.fetchQ("plans", "");
    return plans.running.find((p: any) => p.id === "nt-90").today.chapters[0];
  });

  // Tap → today's first unread chapter, in the reader.
  await chip.click();
  expect(await page.evaluate(() => {
    const p = (window as any).__plumbline.panes[0];
    return { book: p.book, chapter: p.chapter };
  })).toEqual({ book: first.book, chapter: first.chapter });

  // The passage navigator leads with the today card; its chapter buttons GO.
  await page.locator(".pane .nav .passage").first().click();
  const card = page.locator('[data-surface="plan-today"]');
  await expect(card).toBeVisible();
  await expect(card).toContainText("Day 1");
  await expect(card).toContainText("The New Testament in 90 days");
  await card.getByRole("button", { name: first.display, exact: false }).first().click();
  await expect(card).toHaveCount(0); // the dialog closed with the navigation
  expect(await page.evaluate(() => (window as any).__plumbline.panes[0].chapter)).toBe(first.chapter);

  // In concept-study mode the chip stands down: the tracker is suspended, so
  // schedule reading in the mode would earn no credit.
  await page.evaluate(async () => {
    await (window as any).__plumbline.startConceptStudy("grace");
  });
  await expect(page.locator(".plan-chip-row")).toHaveCount(0);
  await page.getByRole("button", { name: "Exit Concept Study" }).click();
  await expect(chip).toBeVisible();
});
