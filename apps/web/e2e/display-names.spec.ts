import { expect, test, type Page } from "@playwright/test";

// A reader is told the book's name; the wire keeps the OSIS id.
//
// These are two different strings for the same verse and the app needs both. The
// refKey ("1Cor 13:4") is frozen — it is what the engine takes, what is written
// into the tag and thread files on disk, and what `?at=` carries. The display name
// ("1 Corinthians 13:4") is what a person says out loud. Until 2026-07-30 (audit
// D-13) the web shell used the refKey for both, so its sheets and toasts read
// "Tag 1Cor 13:4" while Android — the UX gold standard — has named books in full
// since it shipped.
//
// Each test therefore asserts BOTH HALVES on one action, because either half alone
// is satisfied by a bug:
//   1. the sheet heading and the toast say "1 Corinthians 13:4";
//   2. what landed in the study file is still "1Cor 13:4".
// A shell that translated on the way IN would pass (1) and break restore-from-
// backup, refKey lookups and every share link; a shell that never translates
// passes (2) and reads like a database.
//
// NUMBERED BOOKS on purpose. "1Cor" is where the two forms differ most — the
// display name has a space in it, so a helper that split on the FIRST space, or
// one asked to translate a string twice, produces "1 Corinthians" as a book id
// and looks up nothing (the failure e2e/numbered-books.spec.ts guards on the
// navigation side).
//
// NOT RUN by the agent that wrote this file — no Playwright in that sandbox. See
// the report's mutation recipes; the orchestrator runs them.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

const REF = "1Cor 13:4";
const SHOWN = "1 Corinthians 13:4";

test("the tag sheet names the book in full, and the tag file still holds the refKey", async ({ page }) => {
  await boot(page);
  await page.evaluate((ref) => ((window as any).__plumbline.tagPickFor = ref), REF);

  // Scoped to the sheet: `h2` and "Add" both exist elsewhere on screen.
  const sheet = page.locator('[data-surface="tag picker"]');
  await expect(sheet).toBeVisible({ timeout: 15_000 });
  await expect(sheet.locator("h2"), "the tag sheet is reading out an OSIS id").toHaveText(`Tag ${SHOWN}`);

  const name = "Charity";
  await sheet.getByPlaceholder("New tag…").fill(name);
  await sheet.getByRole("button", { name: "＋", exact: true }).click();

  // The toast is the other half of the sentence the sheet started.
  await expect(page.locator(".toast"), "the toast is reading out an OSIS id").toContainText(`Tagged ${SHOWN}`);

  // And the verse the tag now holds is the frozen compact form — read back
  // through the engine, not through the shell's cache, so this is the file.
  const members = async () =>
    ((await page.evaluate(() => (window as any).__plumbline.rpc.call("tags"))) as any).tags
      .find((t: any) => t.name === name)
      ?.members.map((m: any) => m.verse);
  await expect
    .poll(members, { message: "the tag must store the refKey, whatever the sheet showed", timeout: 15_000 })
    .toEqual([REF]);
});

test("the thread sheet names the book in full, and the thread still holds the refKey", async ({ page }) => {
  await boot(page);
  await page.evaluate((ref) => ((window as any).__plumbline.threadPickFor = ref), REF);

  const sheet = page.locator('[data-surface="thread picker"]');
  await expect(sheet).toBeVisible({ timeout: 15_000 });
  await expect(sheet.locator("h2"), "the thread sheet is reading out an OSIS id").toHaveText(
    `Add ${SHOWN} to a thread`,
  );

  const name = "The greater gifts";
  await sheet.getByPlaceholder("New thread…").fill(name);
  await sheet.getByRole("button", { name: "＋", exact: true }).click();

  await expect(page.locator(".toast"), "the toast is reading out an OSIS id").toContainText(`Added ${SHOWN} to`);

  const entries = async () =>
    ((await page.evaluate(() => (window as any).__plumbline.rpc.call("threads"))) as any).threads
      .find((t: any) => t.name === name)
      ?.entries.map((e: any) => e.verse);
  await expect
    .poll(entries, { message: "the thread must store the refKey, whatever the sheet showed", timeout: 15_000 })
    .toEqual([REF]);
});

test("the verse menu's Memorize toast names the book in full, and the card keeps the refKey", async ({ page }) => {
  await boot(page);
  // Driven through session state, as e2e/share-verse.spec.ts drives the same menu:
  // a long-press has to land on a word rectangle inside the canvas, and this file
  // is about the words, not about hit-testing.
  await page.evaluate((ref) => {
    (window as any).__plumbline.contextMenu = { x: 40, y: 180, refKey: ref };
  }, REF);

  const menu = page.locator(".menu");
  await expect(menu.locator(".ref"), "the verse menu is reading out an OSIS id").toHaveText(SHOWN);
  await menu.getByRole("button", { name: "Memorize this verse" }).click();

  await expect(page.locator(".toast"), "the toast is reading out an OSIS id").toContainText(`Memorizing ${SHOWN}`);

  const cards = async () =>
    (
      (await page.evaluate(() =>
        (window as any).__plumbline.rpc.call("memoryCoverage", new Date().toISOString()),
      )) as any
    ).cards.map((c: any) => c.ref);
  await expect
    .poll(cards, { message: "the memorize card must store the refKey, whatever the toast said", timeout: 15_000 })
    .toEqual([REF]);
});
