import { expect, test, type Page } from "@playwright/test";

// The Study hub has to CHANGE with the reader's own study, which is the whole
// point of the band (maintainer, 2026-08-13: "every time I click study it just
// doesn't excite me… a bunch of boring brown cards"). Before this it was eight
// rectangles of fixed text — identical on day one and after a year — so the
// only test that means anything is one that puts real study data in and checks
// the screen reflects it.
//
// Every number here is a query some other screen already makes, arriving
// through the session cache. That is what makes the failure mode worth pinning:
// a count that is fetched but never invalidated looks perfect until the reader
// writes something, so the assertions below WRITE first and then look.

const DESKTOP = { width: 1100, height: 900 };

async function boot(page: Page): Promise<void> {
  await page.setViewportSize(DESKTOP);
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
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
  // Day one of the whole-Bible plan is Genesis 1–4, and the hub says so rather
  // than making the reader open Plans to find out.
  await expect(band).toContainText("The whole Bible in a year", { timeout: 15_000 });
  // `chapterSpan` collapses a run into one span, so a Gen 1–4 day reads
  // "Genesis 1–4" rather than four names — day-numbered like the chip, so a
  // reader working ahead can see the day advance (UAT, 2026-08-18).
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

  // The band answers "what needs reading", and a paused plan needs nothing —
  // the same rule the nav-strip chip follows. Its row is absent rather than
  // present-and-greyed: naming chapters it is not asking for would read as
  // asking for them.
  const band = page.locator(".band");
  await expect(band).toContainText("In progress");
  await expect(band).not.toContainText("The whole Bible in a year", { timeout: 15_000 });
  await expect(band).not.toContainText("Genesis 1");
});

// The defect this pins: the band read `running[0]`, so a reader with three
// schedules saw one of them and no sign the others existed — while the chip
// two screens away correctly said "+2 more". Every plan gets a row, in order,
// each naming what IT still wants (maintainer, 2026-08-13).
test("every running plan gets its own row, in order", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    // Three DIFFERENT classes, which is what lets them run at once: starting a
    // plan only replaces its own class occupant (core::plan CLASS_*).
    await s.engine.planStart("bible-365", "2026-08-13T12:00:00Z");
    await s.engine.planStart("nt-90", "2026-08-13T12:00:00Z");
    await s.engine.planStart("psalms-proverbs-30", "2026-08-13T12:00:00Z");
  });
  await openStudy(page);

  const rows = page.locator(".band .row");
  await expect.poll(async () => rows.count(), { timeout: 15_000 }).toBe(3);

  // Each row names its own plan AND its own chapters — the whole-Bible plan
  // opens in Genesis, the NT plan in Matthew, the devotional in Psalms. Three
  // plans, three different books: this is the assertion `running[0]` failed.
  const text = await page.locator(".band").innerText();
  expect(text).toContain("The whole Bible in a year\nDay 1 · Genesis 1\u20134");
  expect(text).toContain("The New Testament in 90 days\nDay 1 · Matthew 1\u20134");
  expect(text).toContain("Psalms & Proverbs in a month\nDay 1 · Psalms 1\u20139");
});

// A concept study is a plan in the same list, but it is not a SCHEDULE: no day,
// no chapters, and no builtin — so the old `running[0]` read could put its raw
// id on screen as though it were a plan name.
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

  // WAIT FOR THE COUNTS TO LAND BEFORE CLAIMING ANYTHING IS ZERO. `q` answers
  // null on its first call and fills the cache behind it, so "no chip yet" and
  // "nothing in this tool" look identical for the first frames — an assertion
  // made too early passes against a screen that has not loaded. The bundled
  // stock set seeds weaves, so that chip appearing is the honest signal that
  // these queries have resolved.
  await expect.poll(async () => countOn(page, "Weaves"), { timeout: 30_000 }).not.toBeNull();

  // NOW zero means zero: user notes are never seeded, so this card genuinely
  // holds nothing, and an empty tool reads as quiet rather than as a score of
  // nought.
  expect(await countOn(page, "Notes")).toBeNull();

  await page.evaluate(async () => {
    await (window as any).__plumbline.engine.userNoteSet("John 3:16", "hub probe", "2026-08-13T00:00:00Z");
  });
  // The chip appears WITHOUT a reload: an authoring write invalidates the cache
  // these read through, which is the half a fetched-once count misses.
  await expect.poll(async () => countOn(page, "Notes"), { timeout: 15_000 }).toBe("1");
  await page.evaluate(async () => {
    await (window as any).__plumbline.engine.userNoteSet("Rom 8:28", "second", "2026-08-13T00:00:00Z");
  });
  await expect.poll(async () => countOn(page, "Notes"), { timeout: 15_000 }).toBe("2");

  // Threads ARE seeded by the stock set, so the assertion is a DELTA rather
  // than an absolute — hardcoding the stock count here would make this test a
  // hostage of the bundled study set.
  const before = Number(await countOn(page, "Threads")) || 0;
  await page.evaluate(async () => {
    await (window as any).__plumbline.engine.threadAdd("Sermon: grace", "John 3:16", null, "2026-08-13T00:00:00Z");
  });
  await expect.poll(async () => Number(await countOn(page, "Threads")), { timeout: 15_000 }).toBe(before + 1);
});

// Visualizations is a DOOR, not a branch. It expanded in place at first, with
// the two maps as indented sub-cards, and that tree was the odd one out in a
// shell where a destination replaces what came before (maintainer, 2026-08-13).
// The distinction an inline expansion would fail: the hub's own cards have to
// be GONE while the page is up, and ‹ has to come back to the hub rather than
// to the reader.
test("Visualizations opens a page of its own, and ‹ returns to the hub", async ({ page }) => {
  await boot(page);
  await openStudy(page);
  await expect(page.getByRole("button", { name: /^Reading plans/ })).toBeVisible();

  await page.getByRole("button", { name: /^Visualizations/ }).click();

  // A page: its own bar, and the hub it came from is no longer on screen.
  await expect(page.locator(".bar h2")).toHaveText("Visualizations");
  await expect(page.getByRole("button", { name: /^Reading plans/ })).toHaveCount(0);
  // Both maps, each with its full description rather than an indented line.
  await expect(page.getByRole("button", { name: /^Constellation/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Weave map/ })).toBeVisible();

  // ‹ goes UP ONE LAYER, to Study — not back to the reader.
  await page.locator(".bar .back").click();
  await expect(page.locator(".bar h2")).toHaveText("Study");
  await expect(page.getByRole("button", { name: /^Reading plans/ })).toBeVisible();

  // And Escape agrees with the ‹: same page, same parent. It used to jump
  // straight to the reader, so the same screen had two back affordances with
  // two different answers (three, counting the phone's Back button — see
  // routing.spec.ts "peels one layer at a time"). All three climb one ladder
  // now (Session.popOneLayer). Mutation: in popOneLayer, route "viz" through
  // `goRead()` instead of "explore" → red here on the bar reading "Study".
  await page.getByRole("button", { name: /^Visualizations/ }).click();
  await expect(page.locator(".bar h2")).toHaveText("Visualizations");
  await page.keyboard.press("Escape");
  await expect(page.locator(".bar h2")).toHaveText("Study");
  await expect(page.getByRole("button", { name: /^Reading plans/ })).toBeVisible();
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
  // …and the bar is filled to match, not merely relabelled.
  const width = await page.locator(".cov-fill").evaluate((el) => (el as HTMLElement).style.width);
  expect(Number.parseFloat(width)).toBeGreaterThan(3);
  expect(Number.parseFloat(width)).toBeLessThan(6);
});
