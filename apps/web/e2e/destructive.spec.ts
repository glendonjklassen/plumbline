import { expect, test, type Page } from "@playwright/test";

// Nothing is destroyed without asking, and saying no destroys nothing.
//
// A CLASS test, for the same reason as e2e/surfaces.spec.ts: the app had four
// different answers to "does this ask first?" (2026-07-29). Deleting a memorize
// card asked nothing. Rejecting a suggested weave asked nothing. Untagging asked
// nothing. Deleting a thread asked, via a prompt built by hand at its own call
// site. Whether an action asks should be a property of the action, not of whoever
// wrote its button — so there is one mechanism (`session.askConfirm`) and this
// checks every caller of it.
//
// Each case asserts BOTH halves, because a confirmation that appears and then
// deletes anyway is worse than none:
//   1. the confirmation appears;
//   2. Cancel leaves the thing intact;
//   3. the destructive button actually destroys it.

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

const confirm = (page: Page) => page.locator('[data-surface="confirm"]');

test("deleting a thread asks, and Cancel keeps it", async ({ page }) => {
  await boot(page);

  // A thread to delete. The stock set seeds "Romans Road", so this uses its own.
  await page.evaluate(() =>
    (window as any).__plumbline.author("threadAdd", "Doomed", "John 3:16", null, new Date().toISOString()),
  );
  const names = async () =>
    ((await page.evaluate(() => (window as any).__plumbline.rpc.call("threads"))) as any).threads.map(
      (t: any) => t.name,
    );
  expect(await names()).toContain("Doomed");

  await page.evaluate(() => ((window as any).__plumbline.threadPickFor = "John 3:16"));
  await page.locator(".row", { hasText: "Doomed" }).locator("button.del").click();

  await expect(confirm(page)).toBeVisible();
  await expect(confirm(page)).toContainText("Doomed");
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  await expect(confirm(page)).toBeHidden();
  expect(await names(), "Cancel must not delete").toContain("Doomed");

  // Now mean it.
  await page.locator(".row", { hasText: "Doomed" }).locator("button.del").click();
  await confirm(page).getByRole("button", { name: "Delete thread" }).click();
  await expect.poll(names, { timeout: 15_000 }).not.toContain("Doomed");
});

test("removing a memorize card asks, and Cancel keeps it", async ({ page }) => {
  await boot(page);

  await page.evaluate(() =>
    (window as any).__plumbline.author("memoryAdd", "John 3:16", new Date().toISOString()),
  );
  const cards = async () =>
    ((await page.evaluate(() =>
      (window as any).__plumbline.rpc.call("memoryCoverage", new Date().toISOString()),
    )) as any).cards.map((c: any) => c.ref);
  await expect.poll(cards, { timeout: 15_000 }).toContain("John 3:16");

  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.screen = "memorize";
    s.memorize = { view: "hub" };
  });
  const remove = page.locator("button.remove").first();
  await expect(remove).toBeVisible({ timeout: 15_000 });

  await remove.click();
  await expect(confirm(page), "removing a card must ask — it takes the review log with it").toBeVisible();
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  expect(await cards(), "Cancel must not remove the card").toContain("John 3:16");

  await page.locator("button.remove").first().click();
  await confirm(page).getByRole("button", { name: "Remove card" }).click();
  await expect.poll(cards, { timeout: 15_000 }).not.toContain("John 3:16");
});

test("clearing a chapter's reading history asks", async ({ page }) => {
  await boot(page);

  // A by-hand date is the one thing here that nothing else records, so losing it
  // to a stray tap is unrecoverable.
  await page.evaluate(() =>
    (window as any).__plumbline.author("readingMarkRead", "Gen", 1, "2026-01-01"),
  );
  const lastRead = async () =>
    ((await page.evaluate(() =>
      (window as any).__plumbline.rpc.call("readingChapters", "Gen", new Date().toISOString()),
    )) as any).chapters.find((c: any) => c.chapter === 1)?.lastRead ?? null;
  await expect.poll(lastRead, { timeout: 15_000 }).toBe("2026-01-01");

  await page.evaluate(() => ((window as any).__plumbline.markReadFor = { book: "Gen", chapter: 1 }));
  await page.getByRole("button", { name: "Clear history" }).click();
  await expect(confirm(page)).toBeVisible();
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  expect(await lastRead(), "Cancel must not clear it").toBe("2026-01-01");

  await page.getByRole("button", { name: "Clear history" }).click();
  await confirm(page).getByRole("button", { name: "Clear history" }).click();
  await expect.poll(lastRead, { timeout: 15_000 }).toBe(null);
});

test("a confirmation always settles — Escape and navigating away both mean no", async ({ page }) => {
  await boot(page);

  // `askConfirm` hands out a promise. If a dismissal path forgets to resolve it,
  // the caller waits forever and the action silently never happens — which looks
  // like a dead button rather than a bug, so it is worth pinning.
  const escaped = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const p = s.askConfirm("Escape?", "body", "Do it");
    await new Promise((r) => setTimeout(r, 50));
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    return Promise.race([p, new Promise((r) => setTimeout(() => r("HUNG"), 3000))]);
  });
  expect(escaped, "Escape resolves the promise as a no").toBe(false);

  const navigated = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const p = s.askConfirm("Navigate?", "body", "Do it");
    await new Promise((r) => setTimeout(r, 50));
    s.dismissTransient();
    return Promise.race([p, new Promise((r) => setTimeout(() => r("HUNG"), 3000))]);
  });
  expect(navigated, "navigating away resolves the promise as a no").toBe(false);
});
