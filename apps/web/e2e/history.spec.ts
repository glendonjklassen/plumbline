import { expect, test, type Page } from "@playwright/test";

// The reading-history sheet groups a run of contiguous chapters into one line:
// an evening in Genesis 1, 2 and 3 is one thing the reader did, and three lines
// of it pushed everything else off the sheet (maintainer, 2026-08-13).
//
// The grouping lives in BOTH shells (`shell/historySpans.ts` here,
// `historySpans` in ui/StudyScreen.kt there) because the shells own this list —
// `config.history` is prepended locally on every navigation and only reaches the
// engine on a debounced save, so anything the core derived would be stale the
// moment the reader turned a page. Android's half is held by HistorySpansTest;
// this is the web's, driven through the sheet a reader actually opens.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1100, height: 800 });
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** Seed the recents list directly — the shell owns it, and driving twelve real
 *  navigations would test the navigator rather than the grouping. A third
 *  element is the named seating the entry was read in. */
async function seed(page: Page, entries: [string, number, string?][]): Promise<void> {
  await page.evaluate((h) => {
    const s = (window as any).__plumbline;
    s.config.history = h.map(([book, chapter, slot]) => ({ book, chapter, ...(slot ? { slot } : {}) }));
  }, entries);
}

async function lines(page: Page): Promise<string[]> {
  await page.evaluate(() => ((window as any).__plumbline.showHistory = true));
  const list = page.locator('[data-surface="history"] .list button');
  await expect(list.first()).toBeVisible({ timeout: 15_000 });
  return list.allInnerTexts();
}

test("a run of contiguous chapters is one line", async ({ page }) => {
  await boot(page);
  // Read Gen 1 → 2 → 3, which lands in a most-recent-first list as 3, 2, 1.
  await seed(page, [["Gen", 3], ["Gen", 2], ["Gen", 1]]);
  expect(await lines(page)).toEqual(["Genesis 1–3"]);
});

test("a tap opens where the reader was, not the lowest chapter in the span", async ({ page }) => {
  await boot(page);
  await seed(page, [["Gen", 3], ["Gen", 2], ["Gen", 1]]);
  await lines(page);
  await page.locator('[data-surface="history"] .list button').first().click();
  // Genesis 3 — the run's most recent entry — not Genesis 1.
  await expect(page.locator(".subtitle")).toHaveText("Genesis 3", { timeout: 30_000 });
});

test("another book breaks the run, even when the chapters would have joined", async ({ page }) => {
  await boot(page);
  // THE RULE THAT MATTERS: adjacency is in the LIST, not merely similarity.
  // Merging Gen 2 into Gen 3 would claim the reader went 2→3 without leaving,
  // when they went to John in between — rewriting the order they did things in.
  await seed(page, [["Gen", 3], ["John", 1], ["Gen", 2]]);
  expect(await lines(page)).toEqual(["Genesis 3", "John 1", "Genesis 2"]);
});

test("gaps stand apart, and several runs each get their own line", async ({ page }) => {
  await boot(page);
  await seed(page, [["Ps", 119], ["Matt", 7], ["Matt", 6], ["Matt", 5], ["Gen", 5], ["Gen", 2], ["Gen", 1]]);
  expect(await lines(page)).toEqual(["Psalms 119", "Matthew 5–7", "Genesis 5", "Genesis 1–2"]);
});

// ── where it was read ──────────────────────────────────────────────────────────
//
// A run read in a named seating carries that seating's icon. This is where the
// Sunday-morning bookmark shows itself now that the reader's bookmarks row no
// longer does (maintainer, 2026-09-21: "it's fine to have history entries
// represent if they were from church or not with a little icon"). The everyday
// seating is the absence of a mark.

test("a run read at church carries the seating's icon; everyday reading carries none", async ({ page }) => {
  await boot(page);
  await seed(page, [["Rom", 8, "sunday-morning"], ["Gen", 5]]);
  expect(await lines(page)).toEqual(["Romans 8", "Genesis 5"]);
  const rows = page.locator('[data-surface="history"] .list button');
  const icon = rows.nth(0).locator("svg.seat");
  await expect(icon).toHaveAttribute("data-slot", "sunday-morning");
  // The seating's name is the icon's accessible name — a screen reader hears
  // "Sunday morning" where a sighted reader sees the sun.
  await expect(icon).toHaveAttribute("aria-label", "Sunday morning");
  await expect(rows.nth(1).locator("svg.seat")).toHaveCount(0);
});

// Can fail against a grouping that ignores the seating: Genesis 2 would join
// Genesis 3's run, and the sun on that one line would claim Saturday night's
// chapter for church.
test("a seating breaks a run: Saturday night's chapter is not filed under Sunday", async ({ page }) => {
  await boot(page);
  await seed(page, [["Gen", 3, "sunday-morning"], ["Gen", 2]]);
  expect(await lines(page)).toEqual(["Genesis 3", "Genesis 2"]);
});

// The stamp itself, through real navigations: the seating the session is in at
// the moment goes on the entry, and the everyday one writes no key at all — a
// reader who never sits in a named seating keeps writing the file they always
// did.
test("a navigation stamps the seating it was made in", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    // Set by hand and navigated in the same tick, before the save's own re-ask
    // of the engine can answer and put the real seating back.
    s.slot = "sunday-morning";
    s.navigate(0, "Ps", 23);
    s.slot = "other";
    s.navigate(0, "Gen", 1);
  });
  expect(await page.evaluate(() => (window as any).__plumbline.config.history.slice(0, 2))).toEqual([
    { book: "Gen", chapter: 1 },
    { book: "Ps", chapter: 23, slot: "sunday-morning" },
  ]);
});
