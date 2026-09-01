import { expect, test, type Page } from "@playwright/test";

// The Study hub has to change with the reader's own study, so these tests put real study
// data in and check the screen reflects it.
//
// Every number here is a query some other screen already makes, arriving through the session
// cache. A count that is fetched but never invalidated looks perfect until the reader writes
// something, so the assertions below write first and then look.

const DESKTOP = { width: 1100, height: 900 };

async function boot(page: Page): Promise<void> {
  await page.setViewportSize(DESKTOP);
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

const openStudy = (page: Page) => page.getByRole("button", { name: "Study" }).first().click();

/** The count chip on a named card, or null when the card shows none. */
async function countOn(page: Page, cardName: string): Promise<string | null> {
  const chip = page.locator(".ex-card", { hasText: cardName }).first().locator(".ex-count");
  return (await chip.count()) ? chip.innerText() : null;
}

test("the band carries the running plan and today's chapters", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    await (window as any).__plumbline.engine.planStart("bible-365", "2026-08-13T12:00:00Z");
  });
  await openStudy(page);

  const band = page.locator(".band");
  await expect(band).toContainText("In progress");
  // Day one of the whole-Bible plan is Genesis 1–4, and the hub says so rather than making
  // the reader open Plans to find out.
  await expect(band).toContainText("The whole Bible in a year", { timeout: 15_000 });
  // `chapterSpan` collapses a run into one span, so a Gen 1–4 day reads "Genesis 1–4"
  // rather than four names — day-numbered like the chip, so a reader working ahead can see
  // the day advance.
  await expect(band).toContainText("Day 1 · Genesis 1–4");
});

test("a paused plan asks for nothing, so it is not in the band at all", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.planStart("bible-365", "2026-08-13T12:00:00Z");
    await s.engine.planSetPaused("bible-365", true);
  });
  await openStudy(page);

  // The band answers "what needs reading" and a paused plan needs nothing, so its row is
  // absent rather than present-and-greyed: naming chapters it is not asking for would read
  // as asking for them.
  const band = page.locator(".band");
  await expect(band).toContainText("In progress");
  await expect(band).not.toContainText("The whole Bible in a year", { timeout: 15_000 });
  await expect(band).not.toContainText("Genesis 1");
});

// The band read `running[0]`, so a reader with three schedules saw one of them and no sign
// the others existed. Every plan gets a row, in order, each naming what it still wants.
test("every running plan gets its own row, in order", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    // Three different classes, which is what lets them run at once: starting a plan only
    // replaces its own class occupant (core::plan CLASS_*).
    await s.engine.planStart("bible-365", "2026-08-13T12:00:00Z");
    await s.engine.planStart("nt-90", "2026-08-13T12:00:00Z");
    await s.engine.planStart("psalms-proverbs-30", "2026-08-13T12:00:00Z");
  });
  await openStudy(page);

  const rows = page.locator(".band .row");
  await expect.poll(async () => rows.count(), { timeout: 15_000 }).toBe(3);

  // Each row names its own plan and its own chapters: three plans, three different books —
  // the assertion `running[0]` failed.
  const text = await page.locator(".band").innerText();
  expect(text).toContain("The whole Bible in a year\nDay 1 · Genesis 1\u20134");
  expect(text).toContain("The New Testament in 90 days\nDay 1 · Matthew 1\u20134");
  expect(text).toContain("Psalms & Proverbs in a month\nDay 1 · Psalms 1\u20139");
});

// A concept study is a plan in the same list but not a schedule: no day, no chapters, no
// builtin — so the old `running[0]` read could put its raw id on screen as a plan name.
test("a concept study is not a schedule, and never appears in the band", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.planStart("bible-365", "2026-08-13T12:00:00Z");
    await s.engine.conceptStudyStart("grace", "2026-08-13T12:00:00Z");
  });
  await openStudy(page);

  const rows = page.locator(".band .row");
  await expect.poll(async () => rows.count(), { timeout: 15_000 }).toBe(1);
  await expect(page.locator(".band")).toContainText("The whole Bible in a year");
});

test("the tool cards count what is in them, and stay quiet at zero", async ({ page }) => {
  await boot(page);
  await openStudy(page);

  // Wait for the counts to land before claiming anything is zero: `q` answers null on its
  // first call and fills the cache behind it, so "no chip yet" and "nothing in this tool"
  // look identical for the first frames. The stock set seeds weaves, so that chip appearing
  // is the signal that these queries have resolved.
  await expect.poll(async () => countOn(page, "Weaves"), { timeout: 30_000 }).not.toBeNull();

  // Now zero means zero: user notes are never seeded, so an empty tool reads as quiet
  // rather than as a score of nought.
  expect(await countOn(page, "Notes")).toBeNull();

  await page.evaluate(async () => {
    await (window as any).__plumbline.engine.userNoteSet("John 3:16", "hub probe", "2026-08-13T00:00:00Z");
  });
  // The chip appears without a reload: an authoring write invalidates the cache these read
  // through, which is the half a fetched-once count misses.
  await expect.poll(async () => countOn(page, "Notes"), { timeout: 15_000 }).toBe("1");
  await page.evaluate(async () => {
    await (window as any).__plumbline.engine.userNoteSet("Rom 8:28", "second", "2026-08-13T00:00:00Z");
  });
  await expect.poll(async () => countOn(page, "Notes"), { timeout: 15_000 }).toBe("2");

  // Threads are seeded by the stock set, so this asserts a delta — hardcoding the stock
  // count would make the test a hostage of the bundled study set.
  const before = Number(await countOn(page, "Threads")) || 0;
  await page.evaluate(async () => {
    await (window as any).__plumbline.engine.threadAdd("Sermon: grace", "John 3:16", null, "2026-08-13T00:00:00Z");
  });
  await expect.poll(async () => Number(await countOn(page, "Threads")), { timeout: 15_000 }).toBe(before + 1);
});

// Visualizations is a door, not a branch: a destination replaces what came before. The
// distinction an inline expansion would fail is that the hub's own cards have to be gone
// while the page is up, and ‹ has to come back to the hub rather than to the reader.
test("Visualizations opens a page of its own, and ‹ returns to the hub", async ({ page }) => {
  await boot(page);
  await openStudy(page);
  await expect(page.getByRole("button", { name: /^Devotionals and reading plans/ })).toBeVisible();

  await page.getByRole("button", { name: /^Visualizations/ }).click();

  // A page: its own bar, and the hub it came from is no longer on screen.
  await expect(page.locator(".bar h2")).toHaveText("Visualizations");
  await expect(page.getByRole("button", { name: /^Devotionals and reading plans/ })).toHaveCount(0);
  // Both maps, each with its full description rather than an indented line.
  await expect(page.getByRole("button", { name: /^Constellation/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Weave map/ })).toBeVisible();

  // ‹ goes up one layer, to Study — not back to the reader.
  await page.locator(".bar .back").click();
  await expect(page.locator(".bar h2")).toHaveText("Study");
  await expect(page.getByRole("button", { name: /^Devotionals and reading plans/ })).toBeVisible();

  // Escape agrees with the ‹: same page, same parent. All three back affordances — ‹,
  // Escape and the phone's Back button (routing.spec.ts) — climb one ladder, in
  // Session.popOneLayer. Mutation: route "viz" there through `goRead()` instead of
  // "explore" → red on the bar reading "Study".
  await page.getByRole("button", { name: /^Visualizations/ }).click();
  await expect(page.locator(".bar h2")).toHaveText("Visualizations");
  await page.keyboard.press("Escape");
  await expect(page.locator(".bar h2")).toHaveText("Study");
  await expect(page.getByRole("button", { name: /^Devotionals and reading plans/ })).toBeVisible();
});

// Weaves is a door too: the hub spent two sibling cards — Weaves and Suggested — on two views
// of the same library, so both live on one page. The band's "to review" row is the only
// suggested surface left on the hub itself.
test("Weaves opens a page holding the library and the review queue", async ({ page }) => {
  await boot(page);
  await openStudy(page);
  // The stock set seeds weaves, so the count landing means the hub settled.
  await expect.poll(async () => countOn(page, "Weaves"), { timeout: 30_000 }).not.toBeNull();
  await expect(page.locator(".ex-card", { hasText: /^Suggested/ }), "Suggested is no longer a hub card").toHaveCount(0);

  await page.locator(".ex-card", { hasText: /^Weaves/ }).click();
  await expect(page.locator(".bar h2")).toHaveText("Weaves");
  await expect(page.locator(".ex-card", { hasText: /^Review suggested/ })).toBeVisible();

  // Browse raises the library panel the hub card used to raise directly.
  await page.locator(".ex-card", { hasText: /^Browse weaves/ }).click();
  await expect(page.locator("aside.panel")).toContainText("Weaves");

  // Escape climbs the same ladder: panel first, then the page up to its parent hub.
  // Mutation: drop "weaves" from the sub-page rung in Session.popOneLayer → the second
  // press lands in the reader, and the bar below reads Genesis instead of Study.
  await page.keyboard.press("Escape");
  await expect(page.locator("aside.panel")).toHaveCount(0);
  await expect(page.locator(".bar h2")).toHaveText("Weaves");
  await page.keyboard.press("Escape");
  await expect(page.locator(".bar h2")).toHaveText("Study");
});

test("coverage counts the chapters actually read", async ({ page }) => {
  await boot(page);
  await openStudy(page);

  const coverage = page.locator(".coverage");
  // The canon's own size, so the row means something before anything is read.
  await expect(coverage).toContainText("1,189 chapters", { timeout: 15_000 });
  await expect(coverage).toContainText("0 of");

  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    for (let c = 1; c <= 50; c++) await s.engine.readingMarkRead("Gen", c, "2026-08-10T12:00:00Z");
  });

  await expect(coverage).toContainText("50 of 1,189 chapters", { timeout: 15_000 });
  // The bar is filled to match, not merely relabelled.
  const width = await page.locator(".cov-fill").evaluate((el) => (el as HTMLElement).style.width);
  expect(Number.parseFloat(width)).toBeGreaterThan(3);
  expect(Number.parseFloat(width)).toBeLessThan(6);
});
