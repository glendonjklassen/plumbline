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
  await expect(band).toContainText("Genesis 1");
  await expect(band).toContainText("Genesis 4");
});

test("a paused plan asks for nothing, and says so", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.planStart("bible-365", "2026-08-13T12:00:00Z");
    await s.engine.planSetPaused("bible-365", true);
  });
  await openStudy(page);

  const band = page.locator(".band");
  await expect(band).toContainText("paused", { timeout: 15_000 });
  // Promising chapters it is not asking for would read as asking for them —
  // the same call the Plans screen makes.
  await expect(band).not.toContainText("Genesis 1");
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
