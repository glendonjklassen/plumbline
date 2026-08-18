import { readFileSync } from "node:fs";
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

/** Open the search SCREEN from the app bar, and wait for its field. */
async function openSearch(page: Page) {
  await page.getByLabel("Open search").click();
  const field = page.getByLabel("Search", { exact: true });
  await expect(field).toBeFocused();
  return field;
}

/** Record every scoped-search query that reaches the engine, in order. */
async function watchSearches(page: Page): Promise<void> {
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    const asked: string[] = [];
    (window as any).__asked = asked;
    const call = s.rpc.call.bind(s.rpc);
    s.rpc.call = (method: string, ...args: unknown[]) => {
      if (method === "searchBlocksScoped") asked.push(String(args[0]));
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

  // The ⌕ is the way into the search SCREEN now, at every width, and the
  // screen focuses its own field on arrival.
  const field = await openSearch(page);

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
    for (const q of queries) await s.fetchQ("searchBlocksScoped", q, "all");
    // ONE synchronous pass, so no async fill can land between the reads.
    const held = queries.filter((q) => s.q("searchBlocksScoped", q, "all") !== null);
    return {
      held,
      asked: queries.length,
      cap: s.constructor.PER_METHOD_CAP.searchBlocksScoped as number,
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

// Search is a DESTINATION now, not a field in the app bar over a study sheet.
// The glass used to reveal an input that shared the header row with the
// chapter nav, the spacer and the ≡ — and answered into the 380px study
// sidebar. A phone got a bottom sheet over the text it was searching.
test("the glass opens a search screen, not a field in the bar", async ({ page }) => {
  await page.setViewportSize({ width: 412, height: 915 }); // a Pixel's CSS width
  await boot(page);

  await expect(page.locator("header .search")).toHaveCount(0);
  const field = await openSearch(page);

  // The field is the screen's, and it has the room a query deserves: most of
  // the width, rather than a share of a row it competes for.
  const box = (await field.boundingBox())!;
  expect(box.width / 412).toBeGreaterThan(0.8);

  await field.fill("shepherd");
  await expect(page.getByRole("button", { name: /Psalms 23:1/ }).first()).toBeVisible({ timeout: 30_000 });
  // A result takes the reader to the verse — leaving the screen for the text.
  await page.getByRole("button", { name: /Psalms 23:1/ }).first().click();
  await expect(page.locator(".subtitle")).toHaveText("Psalms 23", { timeout: 30_000 });
});

// The point of the screen: searching a PART of the Bible. "shepherd" is in both
// testaments, so every chip has something to drop.
//
// MUTATION: in SearchScreen.svelte, pass "all" instead of `s.searchScope` to
// searchBlocksScoped. Red: the New Testament chip still reports the whole-Bible
// count.
test("a scope chip narrows the search", async ({ page }) => {
  await boot(page);
  const field = await openSearch(page);
  await field.fill("shepherd");

  const count = async (): Promise<number> => {
    const head = await page.locator('[data-surface="search results"] >> text=/\\d+ results?/').first().textContent();
    return Number(/(\d[\d,]*)/.exec(head ?? "")?.[1]?.replace(/,/g, "") ?? -1);
  };
  await expect.poll(count, { timeout: 30_000 }).toBeGreaterThan(0);
  const everywhere = await count();

  await page.getByRole("button", { name: "New Testament" }).click();
  await expect.poll(count, { timeout: 30_000 }).toBeLessThan(everywhere);
  const nt = await count();

  await page.getByRole("button", { name: "Old Testament" }).click();
  await expect.poll(count, { timeout: 30_000 }).not.toBe(nt);
  const ot = await count();

  // The two halves account for the whole: a scope that quietly widened or
  // dropped verses would not add up.
  expect(ot + nt, `everywhere ${everywhere}, OT ${ot}, NT ${nt}`).toBe(everywhere);
});

// SEARCHING A SELECTION OF CHAPTERS (maintainer, 2026-08-17): the chips answer
// "where I already am", a range answers a question the reader arrived with —
// the Sermon on the Mount, Paul on the law. A span rather than a set of ticks,
// because those questions are contiguous in canon order and a span stays one
// range test in the engine.
//
// MUTATION: in SearchScreen.svelte's `applyRange`, drop the chapters from the
// token (`span:${fromBook}:1:${toBook}:999`). Red: the narrowed count matches
// the whole-book count, because the range stopped meaning the chapters chosen.
test("a custom chapter range narrows the search", async ({ page }) => {
  await boot(page);
  const field = await openSearch(page);
  await field.fill("God");

  const count = async (): Promise<number> => {
    const head = await page.locator('[data-surface="search results"] >> text=/\\d+ results?/').first().textContent();
    return Number(/(\d[\d,]*)/.exec(head ?? "")?.[1]?.replace(/,/g, "") ?? -1);
  };
  await expect.poll(count, { timeout: 30_000 }).toBeGreaterThan(0);
  const everywhere = await count();

  // Genesis as a whole, then Genesis 1–3: the second must be a subset.
  await page.getByRole("button", { name: "Range…" }).click();
  const [fromBook, fromCh, toBook, toCh] = await page.locator(".range-panel select").all();
  await fromBook.selectOption({ label: "Genesis" });
  await fromCh.selectOption("1");
  await toBook.selectOption({ label: "Genesis" });
  await toCh.selectOption("50");
  await page.getByRole("button", { name: "Search this range" }).click();
  await expect.poll(count, { timeout: 30_000 }).toBeLessThan(everywhere);
  const wholeBook = await count();

  await page.getByRole("button", { name: "Range…" }).click();
  await (await page.locator(".range-panel select").all())[3].selectOption("3");
  await page.getByRole("button", { name: "Search this range" }).click();
  await expect.poll(count, { timeout: 30_000 }).toBeLessThan(wholeBook);
  expect(await count()).toBeGreaterThan(0);

  // The chip says what was searched, so the number on screen is explained.
  await expect(page.getByRole("button", { name: /Genesis 1–3/ })).toBeVisible();
});

// The presets are the canon's own sections (reference::CANON_SEGMENTS), the
// same rows the canon strip paints — so a preset can never name a stretch the
// strip draws differently.
//
// MUTATION: build the preset token from `books[seg.first]` twice (first..first
// rather than first..last). Red: "Gospels" returns Matthew's count, not the
// four Gospels'.
test("a canon preset searches its whole section", async ({ page }) => {
  await boot(page);
  const field = await openSearch(page);
  await field.fill("God");

  const count = async (): Promise<number> => {
    const head = await page.locator('[data-surface="search results"] >> text=/\\d+ results?/').first().textContent();
    return Number(/(\d[\d,]*)/.exec(head ?? "")?.[1]?.replace(/,/g, "") ?? -1);
  };
  await expect.poll(count, { timeout: 30_000 }).toBeGreaterThan(0);

  await page.getByRole("button", { name: "Range…" }).click();
  await page.getByRole("button", { name: /Gospels/ }).click();
  await expect.poll(count, { timeout: 30_000 }).toBeGreaterThan(0);
  const gospels = await count();

  // Matthew alone is inside the Gospels, so it must be a strict subset.
  await page.getByRole("button", { name: "Range…" }).click();
  const selects = await page.locator(".range-panel select").all();
  await selects[0].selectOption({ label: "Matthew" });
  await selects[1].selectOption("1");
  await selects[2].selectOption({ label: "Matthew" });
  await selects[3].selectOption("28");
  await page.getByRole("button", { name: "Search this range" }).click();
  await expect.poll(count, { timeout: 30_000 }).toBeLessThan(gospels);
});

// The canon's section names are the reader's language, not English constants.
// They are ids in `reference::CANON_SEGMENTS` — matched by value in memory.rs —
// so the ids stay English and `segment_label` translates what is shown. This
// checks the whole way through: catalogue → engine → the preset a reader taps.
//
// Driven by CATALOGUE KEY rather than by German words typed into the test, the
// discipline language.spec.ts already keeps: the words on those buttons are
// exactly what is under test elsewhere, and hardcoding them here would be a
// second copy to get wrong (it was, on the first attempt).
//
// MUTATION: in wire.rs serve `label` straight from CANON_SEGMENTS instead of
// through `segment_label`. Red: the German reader is offered "Gospels".
const DE_CAT: Record<string, string> = JSON.parse(
  readFileSync(new URL("../../../crates/core/src/i18n/de.json", import.meta.url), "utf8"),
);

test.describe("a German reader", () => {
  test.use({ locale: "de-DE" });

  test("is offered the canon's sections in German", async ({ page }) => {
    await page.goto("/");
    const est = page.getByRole("button", { name: DE_CAT["intro.pathEstablished"] });
    const canvas = page.locator(".pane canvas").first();
    await expect(est.or(canvas)).toBeVisible({ timeout: 90_000 });
    if (await est.isVisible().catch(() => false)) {
      await est.click();
      await page.getByRole("button", { name: DE_CAT["intro.start"] }).click();
    }
    await expect(canvas).toBeVisible({ timeout: 90_000 });

    await page.getByLabel(DE_CAT["common.openSearch"]).click();
    await page.getByRole("button", { name: DE_CAT["search.scopeRange"] }).click();
    // The section names, in German — and the canon strip reads the same eight
    // strings, so this pins both surfaces at once.
    await expect(page.getByRole("button", { name: /Evangelien/ })).toBeVisible({ timeout: 30_000 });
    await expect(page.getByRole("button", { name: /Gesetz/ })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Gospels/ })).toHaveCount(0);
  });
});
