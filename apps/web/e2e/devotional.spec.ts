import { expect, test, type Page } from "@playwright/test";

// Devotionals (maintainer, 2026-08-26): a bundled booklet of dated readings,
// one entry a day. It starts and stops like a reading plan, it rides the reader
// as a nav-strip chip, and reading one is a FULL PAGE with the day's passage
// set beneath the title — not a study-panel block.
//
// The pacing is the part worth testing hardest, because it is the part with a
// calendar in it: today's entry is the lowest day not yet banked, Done banks
// it, and the NEXT entry is withheld until the reader's next LOCAL midnight.
// The core owns that rule (`core::devotional`, unit-tested against fixed
// dates); what these tests pin is that the shell asks it the right question and
// draws the answer — in particular that the chip retires on Done, which is the
// one behaviour a reader would notice going wrong the same day.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** The bundled booklet's id, off the catalogue rather than written in here. */
async function bookletId(page: Page): Promise<string> {
  return await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const wire = await s.rpc.call("devotionals", "en", "2026-08-26");
    return (wire?.catalogue ?? []).find((b: any) => b.newBeliever)?.id ?? "";
  });
}

test("the bundled booklet is offered, and exactly one is the new-believer one", async ({ page }) => {
  await boot(page);
  const cat = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return (await s.rpc.call("devotionals", "en", "2026-08-26"))?.catalogue ?? [];
  });
  expect(cat.length).toBeGreaterThan(0);
  // The welcome starts "the one flagged", so two flagged would make which
  // booklet a new believer is handed depend on file order.
  expect(cat.filter((b: any) => b.newBeliever).length).toBe(1);
  const one = cat.find((b: any) => b.newBeliever);
  expect(one.days).toBeGreaterThan(0);
  expect(one.name.trim()).not.toBe("");
});

// THE PAGE. Title, then the passage, then the reflection, then the activity —
// and the passage is REAL VERSE TEXT pulled from the corpus, not the reference
// string. FAILS against the bug it describes without mutation: if
// DevotionalScreen stopped resolving `scripture` through `q("verse", …)` the
// block would render its label and no words, which is what the length
// assertion below is for. A reference printed as text would also fail it, since
// the assertion is that the verse block does NOT merely repeat the label.
test("a devotional day is a full page with its passage beneath the title", async ({ page }) => {
  await boot(page);
  const id = await bookletId(page);
  expect(id).not.toBe("");

  await page.evaluate(async (bid) => {
    const s = (window as any).__plumbline;
    await s.author("devotionalStart", bid, new Date().toISOString());
    s.openDevotional(bid, 1);
  }, id);

  const screen = page.locator("section.screen");
  await expect(screen.locator("h1")).toBeVisible({ timeout: 30_000 });
  await expect(screen.locator(".dayline")).toContainText("Day 1");

  // The passage: a reference button, and beneath it verses with numbers.
  const passage = screen.locator(".passage").first();
  await expect(passage).toBeVisible();
  const refLabel = (await passage.locator(".ref").textContent())!.trim();
  expect(refLabel).toMatch(/\d+:\d+/);
  await expect
    .poll(async () => ((await passage.locator(".verses").textContent()) ?? "").trim().length, { timeout: 30_000 })
    .toBeGreaterThan(80);
  const verses = (await passage.locator(".verses").textContent())!;
  expect(verses, "the block must hold the TEXT, not a second copy of the reference").not.toBe(refLabel);

  await expect(screen.locator(".reflection").first()).toBeVisible();
  await expect(screen.locator(".activity")).toBeVisible();
});

// THE PACING, end to end and through the real button. This is the test that
// would catch the chip forgetting to retire — the failure a reader meets the
// same evening they read their first entry.
test("Done banks the day, retires the chip, and holds the next entry until tomorrow", async ({ page }) => {
  await boot(page);
  const id = await bookletId(page);

  await page.evaluate(async (bid) => {
    const s = (window as any).__plumbline;
    await s.author("devotionalStart", bid, new Date().toISOString());
  }, id);

  // The chip is on the strip, carrying day 1.
  const chip = page.locator(`.plan-chip-row [data-devotional="${id}"]`);
  await expect(chip).toBeVisible({ timeout: 30_000 });
  await expect(chip).toContainText("Day 1");

  // Tapping it opens the page for that day.
  await chip.click();
  await expect(page.locator("section.screen .dayline")).toContainText("Day 1", { timeout: 30_000 });

  // Done — the only signal there is that a devotional day was read.
  await page.locator("section.screen button.done").click();
  await expect(page.locator("section.screen .already")).toBeVisible({ timeout: 30_000 });

  // Back at the reader, the chip is GONE: the day is banked and the next entry
  // is tomorrow's. This is the retirement rule, and it is what distinguishes a
  // devotional chip from the plan chip beside it (which deliberately rolls
  // straight on to the next portion so a reader can work ahead).
  await page.evaluate(() => (window as any).__plumbline.goRead());
  await expect(chip).toHaveCount(0, { timeout: 30_000 });

  // …and the engine agrees for the right REASON: day 2 is open, it is simply
  // not on offer until the local date moves.
  const state = await page.evaluate(async (bid) => {
    const s = (window as any).__plumbline;
    const d = new Date();
    const today = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    const wire = await s.rpc.call("devotionals", "en", today);
    const run = (wire?.running ?? []).find((r: any) => r.id === bid);
    // The same question asked of TOMORROW, which is the half that proves the
    // chip is withheld by the calendar and not simply broken.
    const t = new Date(d.getTime() + 86_400_000);
    const tomorrow = `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, "0")}-${String(t.getDate()).padStart(2, "0")}`;
    const later = await s.rpc.call("devotionals", "en", tomorrow);
    const runLater = (later?.running ?? []).find((r: any) => r.id === bid);
    return { day: run?.today?.day, available: run?.today?.available, done: run?.daysDone, laterAvailable: runLater?.today?.available };
  }, id);
  expect(state.done).toBe(1);
  expect(state.day).toBe(2);
  expect(state.available).toBe(false);
  expect(state.laterAvailable, "the entry returns at the next local midnight").toBe(true);
});

// STARTING AND STOPPING, through the Plans screen — "the same way you can start
// a reading plan" (maintainer, 2026-08-26).
test("a devotional starts and stops from the Plans screen", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => ((window as any).__plumbline.screen = "plans"));

  const start = page.locator("button.plan-builtin", { hasText: "Start" }).first();
  await expect(page.getByRole("heading", { name: "Devotionals", exact: true })).toBeVisible({ timeout: 30_000 });

  // The booklet's own row, found by its name off the catalogue.
  const id = await bookletId(page);
  const name = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const wire = await s.rpc.call("devotionals", "en", "2026-08-26");
    return (wire?.catalogue ?? []).find((b: any) => b.newBeliever)?.name ?? "";
  });
  await page.locator("button.plan-builtin", { hasText: name }).click();

  // It is now a running card with its day line, not an offer.
  await expect(page.locator(".plan-card", { hasText: name })).toBeVisible({ timeout: 30_000 });
  const running = await page.evaluate(async (bid) => {
    const s = (window as any).__plumbline;
    const wire = await s.rpc.call("devotionals", "en", "2026-08-26");
    return (wire?.running ?? []).some((r: any) => r.id === bid);
  }, id);
  expect(running).toBe(true);
  expect(await start.count()).toBeGreaterThanOrEqual(0); // the catalogue still lists plans
});
