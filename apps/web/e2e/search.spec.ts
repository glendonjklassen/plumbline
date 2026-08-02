import { expect, test, type Page } from "@playwright/test";

// Live search, and the two ways it cost more than it was worth (audit F-12).
//
// The web shell searches per keystroke, which Android does not — its search runs
// on the IME Search action, once (StudyScreen.kt). So every character typed here
// was a full query: four ranked tiers over the whole corpus, then a block list of
// up to 200 hits carrying their verse text, then JSON across the worker boundary.
// The engine lives in ONE thread, so those answers also queue in front of the
// layout and tap RPCs of the chapter underneath.
//
// NEITHER ASSERTION BELOW IS A MILLISECOND BUDGET. The repo has been bitten twice
// by tests whose ceiling a whole un-fixed run still fit inside, so what these
// measure is COUNTS — engine calls against characters typed, cached answers
// against searches run — which mean the same thing on a fast machine and a slow
// one.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/**
 * Wait out the background pipeline, exactly as cache.spec.ts does and for the
 * same reason: `onCoreReady`, `onWarmReady` and `onRndReady` each call the
 * whole-cache `invalidate()`, which would empty the cache underneath a test that
 * is measuring what stayed in it.
 *
 * POLLED FROM NODE. Reading the boot trace is an async RPC, and an async
 * predicate handed to `page.waitForFunction` returns a promise — an object,
 * therefore truthy — so the poller fulfils on its first invocation and waits for
 * nothing. Three files in this suite have shipped that bug.
 */
async function settleBackground(page: Page): Promise<void> {
  const traceLen = () =>
    page.evaluate(async () => ((await (window as any).__plumbline.rpc.bootTrace()) ?? []).length);
  const deadline = Date.now() + 120_000;
  let prev = -1;
  for (;;) {
    const n = await traceLen();
    if (n === prev && n > 10) return;
    if (Date.now() > deadline) {
      throw new Error(`the background pipeline never stopped appending boot-trace entries (${n})`);
    }
    prev = n;
    await new Promise((r) => setTimeout(r, 1500));
  }
}

/** Record every `searchBlocks` query that reaches the engine, in order. */
async function watchSearches(page: Page): Promise<void> {
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    const asked: string[] = [];
    (window as any).__asked = asked;
    const call = s.rpc.call.bind(s.rpc);
    s.rpc.call = (method: string, ...args: unknown[]) => {
      if (method === "searchBlocks") asked.push(String(args[0]));
      return call(method, ...args);
    };
  });
}

// MUTATION: in session.svelte.ts's `setSearch`, replace the body with
// `this.searchDraft = text; this.searchQuery = text;` — the per-keystroke
// behaviour this test exists for. Red: "a typed word must reach the engine as
// one query, not one per keystroke" — received 8 for 8 characters.
test("a burst of keystrokes reaches the engine as one query", async ({ page }) => {
  await boot(page);
  await settleBackground(page);
  await watchSearches(page);

  // The ⌕ button is `display: none` on a wide screen, where the field is always
  // there — it only exists to reveal the field on a narrow one. Clicking it
  // unconditionally hung this test at the default 1280px viewport.
  const glass = page.getByLabel("Open search");
  if (await glass.isVisible()) await glass.click();
  const field = page.getByLabel("Search", { exact: true });
  await field.click();
  await expect(field).toBeFocused();

  const word = "shepherd";
  await page.keyboard.type(word, { delay: 15 });
  expect(await field.inputValue(), "the field itself must never lag the keyboard").toBe(word);

  // The answer the reader is waiting for does arrive.
  await expect
    .poll(() => page.evaluate(() => ((window as any).__asked as string[]).at(-1)), { timeout: 15_000 })
    .toBe(word);
  // Then a beat, so a trailing query cannot slip in after the count is read.
  await page.waitForTimeout(1000);

  const asked: string[] = await page.evaluate(() => (window as any).__asked);
  // The empty string is the panel opening on the first keystroke, before the
  // wait has elapsed. Everything else is a query the reader paused long enough
  // to mean — one, for a word typed straight through. The ceiling is three
  // rather than one so that a main thread stalled past the wait, twice, is not
  // a failure; eight, which is what one-per-keystroke gives, is.
  const real = asked.filter((q) => q !== "");
  expect(
    real.length,
    `a typed word must reach the engine once per pause, not once per keystroke — ` +
      `${word.length} characters produced ${JSON.stringify(real)}`,
  ).toBeLessThanOrEqual(3);
  expect(real.at(-1), "and the query that lands is the whole word").toBe(word);
});

// MUTATION: in session.svelte.ts's `#store`, delete the two lines that look up
// `PER_METHOD_CAP` and call `#trimMethod`. Red: "only the search on screen is
// worth keeping" — received 6 held answers for a cap of 1.
test("the cache keeps one search answer, not one per query", async ({ page }) => {
  await boot(page);
  await settleBackground(page);

  // A search answer is the largest single entry this cache holds — up to 200
  // hits carrying their verse text — and its key is the query string, so before
  // F-12 a reader typing one word left one behind per keystroke, evicting other
  // panels' answers to hold results for fragments nobody will type again.
  const r = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const before = s.cacheSize as number;
    const queries = ["shep", "sheph", "shephe", "shepher", "shepherd", "shepherds"];
    for (const q of queries) await s.fetchQ("searchBlocks", q);
    // ONE synchronous pass, so no async fill can land between the reads.
    const held = queries.filter((q) => s.q("searchBlocks", q) !== null);
    return {
      held,
      asked: queries.length,
      cap: s.constructor.PER_METHOD_CAP.searchBlocks as number,
      grew: (s.cacheSize as number) - before,
      last: queries[queries.length - 1],
    };
  });

  expect(r.asked, "the flood must ask for more searches than the cap").toBeGreaterThan(r.cap);
  expect(r.held.length, "only the search on screen is worth keeping").toBe(r.cap);
  expect(r.held, "and it is the most recent one, not the first").toEqual([r.last]);
  // Search must not be able to push other panels' answers out of a cache whose
  // bound was derived without it.
  expect(r.grew, "six searches must not cost six cache entries").toBeLessThanOrEqual(r.cap);
});
