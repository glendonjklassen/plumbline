import { expect, test, type Page } from "@playwright/test";

// The lifetime counter: how many times this reader has been through the whole
// Bible. Seeded ONCE by hand — somebody arriving with thirty years behind them
// should not start at nought — and EARNED after that: nothing edits it, and the
// only thing that moves it is finishing the canon (maintainer, 2026-08-13).
// The hand count is what came BEFORE the map's own count: the engine counts each
// chapter's reads, the Bible (and each part of it) has been read as often as its
// least-read chapter, and the hand count is added to every part alike
// (maintainer, 2026-10-13).
//
// -1 is "never said", which is deliberately not 0: a reader who answers "none"
// has told us something and must not be asked again.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1100, height: 900 });
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

const openStudy = (page: Page) => page.getByRole("button", { name: "Study" }).first().click();
const reads = (page: Page) => page.evaluate(() => (window as any).__plumbline.config.bibleReads);

test("unset, it asks; answered, it states — and the number sticks", async ({ page }) => {
  await boot(page);
  await openStudy(page);

  // The invitation, not a number.
  const invite = page.locator(".reads.unset");
  await expect(invite).toBeVisible({ timeout: 15_000 });
  await invite.click();

  // A numeric field, which is what raises a phone's numpad.
  const field = page.getByRole("dialog", { name: /How many times/ }).locator("input");
  await expect(field).toHaveAttribute("inputmode", "numeric");
  await field.fill("7");
  await page.getByRole("button", { name: "OK" }).click();

  await expect(page.locator(".reads-n")).toHaveText("7", { timeout: 15_000 });
  expect(await reads(page)).toBe(7);
  // No longer a control: the invitation is gone and there is nothing to retype.
  await expect(page.locator(".reads.unset")).toHaveCount(0);
});

test("nought is an answer, not a refusal to answer", async ({ page }) => {
  await boot(page);
  await openStudy(page);
  await page.locator(".reads.unset").click();
  await page.getByRole("dialog", { name: /How many times/ }).locator("input").fill("0");
  await page.getByRole("button", { name: "OK" }).click();

  // 0 is a real answer and must stick — if "unset" were 0 this would ask again
  // every time, which is the reason the unset value is -1.
  await expect(page.locator(".reads-n")).toHaveText("0", { timeout: 15_000 });
  expect(await reads(page)).toBe(0);
  await expect(page.locator(".reads.unset")).toHaveCount(0);
});

test("cancelling leaves it unasked", async ({ page }) => {
  await boot(page);
  await openStudy(page);
  await page.locator(".reads.unset").click();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.locator(".reads.unset")).toBeVisible();
  expect(await reads(page)).toBe(-1);
});

/** Every chapter of the canon marked read on `day` — the same call the
 *  navigator's press-and-hold makes. */
async function readTheCanon(page: Page, day: string): Promise<void> {
  await page.evaluate(async (day) => {
    const s = (window as any).__plumbline;
    const books = (s.q("toc")?.books ?? []) as any[];
    for (const b of books) {
      for (let c = 1; c <= b.chapters; c++) await s.engine.readingMarkRead(b.id, c, day);
    }
    s.invalidate();
  }, day);
}

test("every finish of the canon counts once, the second as much as the first", async ({ page }) => {
  test.setTimeout(240_000);
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.config.bibleReads = 2;
    s.saveConfig();
  });

  await readTheCanon(page, "2026-08-10T12:00:00Z");
  await openStudy(page);
  await expect(page.locator(".reads-n")).toHaveText("3", { timeout: 60_000 });

  // Look again — and again. The credit is for FINISHING, not for visiting.
  await page.getByRole("button", { name: "Read" }).first().click();
  await openStudy(page);
  await expect(page.locator(".reads-n")).toHaveText("3", { timeout: 30_000 });
  // The hand count is what came before; the map's finish is the map's own.
  expect(await reads(page)).toBe(2);
  // Finished, the bar starts over for the next time through rather than sitting
  // full for good.
  await expect(page.locator(".coverage")).toContainText("0 of 1,189 chapters");

  // The second finish. The old counter credited a finished map once and then
  // never again — every chapter kept its date, so the map never dropped below
  // complete to be finished afresh — and this held at 3.
  await page.getByRole("button", { name: "Read" }).first().click();
  await readTheCanon(page, "2026-09-10T12:00:00Z");
  await openStudy(page);
  await expect(page.locator(".reads-n")).toHaveText("4", { timeout: 60_000 });
});

test("each part of the Bible is counted by its own least-read chapter", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.config.bibleReads = 10;
    s.saveConfig();
    // Every psalm twice, and Psalm 23 once more besides.
    for (const day of ["2026-08-10", "2026-09-10"]) {
      for (let c = 1; c <= 150; c++) await s.engine.readingMarkRead("Ps", c, day);
    }
    await s.engine.readingMarkRead("Ps", 23, "2026-10-01");
    s.invalidate();
  });
  await openStudy(page);

  // Folded until asked for.
  await expect(page.locator(".reads-n")).toHaveText("10", { timeout: 30_000 });
  await expect(page.locator(".sections")).toHaveCount(0);
  await page.locator(".reads").click();
  await expect(page.locator(".reads")).toHaveAttribute("aria-expanded", "true");

  const row = (name: string) => page.locator(".section", { has: page.locator(".section-name", { hasText: new RegExp(`^${name}$`) }) });
  // The hand count lifts every part alike; the map adds what was read here.
  // Psalm 23's extra read does not lift Psalms — it is the way into the third.
  await expect(row("Psalms").locator(".section-n")).toHaveText("12");
  await expect(row("Psalms").locator(".section-next")).toHaveText("1 of 150 chapters toward the next");
  await expect(row("Old Testament").locator(".section-n")).toHaveText("10");
  await expect(row("Wisdom").locator(".section-n")).toHaveText("10");
  await expect(row("Gospels & Acts").locator(".section-n")).toHaveText("10");
  await expect(row("Prophecy").locator(".section-n")).toHaveText("10");
  // Every part, testaments first and their parts under them.
  await expect(page.locator(".section-name")).toHaveText([
    "Old Testament", "Law", "History", "Psalms", "Wisdom", "Prophets",
    "New Testament", "Gospels & Acts", "Letters & Revelation", "Prophecy",
  ]);
});
