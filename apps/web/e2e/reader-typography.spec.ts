import { expect, test, type Page } from "@playwright/test";

// The two reader-typography switches, driven the way a reader drives them:
// through Settings › Advanced, with the assertions on what the READER can see
// rather than on the config value the click just wrote.
//
// They fail in two different ways, which is why they are tested differently:
//
//   * VERSE NUMBERS are a LAYOUT input. The number's box and the gap after it
//     belong to the line, so a shell that merely declined to paint them would
//     flow every verse around an invisible marker. The observable is the
//     display list: no verseNumber items, and the words moved left.
//   * ITALICS are PAINT ONLY — the engine measures every word upright either
//     way (the measure callback is font-blind), so the layout is untouched and
//     the observable is the pixels. Hence a canvas screenshot: the one thing
//     that cannot be satisfied by a flag flipping somewhere.
//
// Both also have to be LIVE. The reader's layout has its own trigger and does
// not track the config, so a toggle that only writes the setting leaves the old
// page on screen until something else happens to re-lay it — the exact bug the
// AKJV overlay shipped with (feedback 2026-07-27, "isn't live").

const DESKTOP = { width: 1100, height: 800 };

async function boot(page: Page): Promise<void> {
  await page.setViewportSize(DESKTOP);
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** Open Settings from the ≡ utilities and expand Advanced, where both switches
 *  live. `details` keeps its open state between visits, so the summary is only
 *  clicked when the block is actually shut. */
async function openAdvanced(page: Page): Promise<void> {
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  const advanced = page.locator('[data-surface="settings"] details.advanced');
  if (!(await advanced.evaluate((d) => (d as HTMLDetailsElement).open)))
    await advanced.locator("> summary").click();
}

/** What the last painted frame actually contains, off the production probe.
 *  `items` is a WeakRef set by the paint itself, so it is null until a frame has
 *  gone up — every read here is polled rather than sampled, or the assertions
 *  race the first paint and pass against an empty list. */
async function painted(page: Page): Promise<{ numbers: number; words: number; firstWordX: number }> {
  return await page.evaluate(() => {
    const items = (window as any).__plumblinePaint?.items?.deref?.() ?? [];
    const words = items.filter((i: any) => i.kind === "word");
    return {
      numbers: items.filter((i: any) => i.kind === "verseNumber").length,
      words: words.length,
      firstWordX: words.length ? words[0].x : -1,
    };
  });
}

/** The config write is debounced (Session.saveConfig, 300 ms) and then crosses
 *  to the worker. The worker is single-threaded and processes messages in
 *  order, so once the debounce has elapsed, awaiting ANY round trip proves the
 *  save that was queued ahead of it has been handled — which is what makes the
 *  reload below a test of persistence rather than a race against it. */
async function settleConfigWrite(page: Page): Promise<void> {
  await page.waitForTimeout(400);
  await page.evaluate(async () => {
    // Any real round trip will do; routeLink is a pure read.
    await (window as any).__plumbline.rpc.static("routeLink", "go:John:3:16");
  });
}

/** Wait until a frame with real text has been painted, and return it. */
async function paintedPage(page: Page): Promise<{ numbers: number; words: number; firstWordX: number }> {
  await expect.poll(async () => (await painted(page)).words, { timeout: 90_000 }).toBeGreaterThan(20);
  return painted(page);
}

test("verse numbers off removes them AND reclaims the space they held", async ({ page }) => {
  await boot(page);

  const before = await paintedPage(page);
  expect(before.numbers, "the shipped reader paints verse numbers").toBeGreaterThan(0);

  await openAdvanced(page);
  await page.getByRole("checkbox", { name: "Verse numbers" }).uncheck();
  await page.keyboard.press("Escape");

  // Live: no reload, no chapter turn, no resize.
  await expect
    .poll(async () => (await painted(page)).numbers, { timeout: 15_000 })
    .toBe(0);

  const after = await painted(page);
  // The same chapter, still all there — this is not an empty page passing by
  // accident.
  expect(after.words).toBe(before.words);
  // And the text moved LEFT into the space the first number held. Without this
  // the numbers could be gone while every verse still started behind a ghost.
  expect(after.firstWordX).toBeLessThan(before.firstWordX);
  expect(after.firstWordX).toBe(0);

  // Back on, and the numbers come back — the setting is a switch, not a door.
  await openAdvanced(page);
  await page.getByRole("checkbox", { name: "Verse numbers" }).check();
  await page.keyboard.press("Escape");
  await expect
    .poll(async () => (await painted(page)).numbers, { timeout: 15_000 })
    .toBe(before.numbers);
});

test("supplied-word italics off changes the painted page, and leaves the layout alone", async ({ page }) => {
  await boot(page);

  // A chapter that actually HAS supplied words, or this test proves nothing.
  // FLAG_ADDED is bit 1 (core::corpus::FLAG_ADDED).
  await paintedPage(page);
  const addedWords = await page.evaluate(() => {
    const items = (window as any).__plumblinePaint?.items?.deref?.() ?? [];
    return items.filter((i: any) => i.kind === "word" && i.flags & 1).length;
  });
  expect(addedWords, "the opening chapter must contain KJV italics for this to mean anything").toBeGreaterThan(0);

  const canvas = page.locator(".pane canvas").first();
  const before = await canvas.screenshot();
  const geometryBefore = await paintedPage(page);

  await openAdvanced(page);
  await page.getByRole("checkbox", { name: "Italicize supplied words" }).uncheck();
  await page.keyboard.press("Escape");

  // The PIXELS change — the italic glyphs are gone. Polled, because this is a
  // repaint rather than a relayout and there is no other signal to wait on.
  await expect
    .poll(async () => (await canvas.screenshot()).equals(before), { timeout: 15_000 })
    .toBe(false);

  // …and the LAYOUT does not. Italics are paint-only: the engine measured these
  // words upright in both states, so a changed box here would mean the toggle
  // had quietly invalidated a layout it has no business touching.
  const geometryAfter = await paintedPage(page);
  expect(geometryAfter).toEqual(geometryBefore);

  // Turning it back on restores the page exactly — painting is deterministic,
  // so anything less would mean state leaked through the toggle.
  await openAdvanced(page);
  await page.getByRole("checkbox", { name: "Italicize supplied words" }).check();
  await page.keyboard.press("Escape");
  await expect
    .poll(async () => (await canvas.screenshot()).equals(before), { timeout: 15_000 })
    .toBe(true);
});

test("both switches survive a reload", async ({ page }) => {
  await boot(page);
  await paintedPage(page);
  await openAdvanced(page);
  await page.getByRole("checkbox", { name: "Verse numbers" }).uncheck();
  await page.getByRole("checkbox", { name: "Italicize supplied words" }).uncheck();
  await page.keyboard.press("Escape");
  await expect.poll(async () => (await painted(page)).numbers, { timeout: 15_000 }).toBe(0);
  await settleConfigWrite(page);

  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });

  // The reader gets back the page they left, not the shipped default.
  await paintedPage(page);
  expect((await painted(page)).numbers).toBe(0);
  expect(
    await page.evaluate(() => {
      const c = (window as any).__plumbline.config;
      return [c.verseNumbers, c.addedItalics];
    }),
  ).toEqual([false, false]);
});
