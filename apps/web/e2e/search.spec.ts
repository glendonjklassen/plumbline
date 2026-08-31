import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";

// Live search, and the two ways it cost more than it was worth. Searching per keystroke
// makes every character a full query: four ranked tiers over the whole corpus, a block
// list of up to 200 hits carrying their verse text, then JSON across the worker
// boundary — and the engine's one thread queues those in front of the layout and tap
// RPCs of the chapter underneath.
//
// Neither assertion below is a millisecond budget, because a whole un-fixed run can fit
// inside a fixed ceiling. They measure counts — engine calls against characters typed,
// cached answers against searches run — which mean the same thing on any machine.

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
 * Wait out the background pipeline: `onCoreReady`, `onWarmReady` and `onRndReady` each
 * call the whole-cache `invalidate()`, which would empty the cache underneath a test
 * measuring what stayed in it.
 *
 * Polled from Node, not with `page.waitForFunction`: reading the boot trace is an async
 * RPC, and an async predicate returns a promise — an object, therefore truthy — so such
 * a poller fulfils on its first invocation and waits for nothing.
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

/** Open the search screen from the app bar, and wait for its field. */
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

// Fails against a `setSearch` that assigns `searchQuery` on every keystroke: 8
// characters reach the engine as 8 queries.
test("a burst of keystrokes reaches the engine as one query", async ({ page }) => {
  await boot(page);
  await settleBackground(page);
  await watchSearches(page);

  const field = await openSearch(page);

  const word = "shepherd";
  await page.keyboard.type(word, { delay: 15 });
  expect(await field.inputValue(), "the field itself must never lag the keyboard").toBe(word);

  await expect
    .poll(() => page.evaluate(() => ((window as any).__asked as string[]).at(-1)), { timeout: 15_000 })
    .toBe(word);
  // Then a beat, so a trailing query cannot slip in after the count is read.
  await page.waitForTimeout(1000);

  const asked: string[] = await page.evaluate(() => (window as any).__asked);
  // The empty string is the panel opening on the first keystroke, before the wait has
  // elapsed. The ceiling is three rather than one so a main thread stalled past the wait
  // twice is not a failure; eight, which one-per-keystroke gives, is.
  const real = asked.filter((q) => q !== "");
  expect(
    real.length,
    `a typed word must reach the engine once per pause, not once per keystroke — ` +
      `${word.length} characters produced ${JSON.stringify(real)}`,
  ).toBeLessThanOrEqual(3);
  expect(real.at(-1), "and the query that lands is the whole word").toBe(word);
});

// Fails against a `#store` that skips `PER_METHOD_CAP` and `#trimMethod`: 6 held
// answers for a cap of 1.
test("the cache keeps one search answer, not one per query", async ({ page }) => {
  await boot(page);
  await settleBackground(page);

  // A search answer is the largest single entry this cache holds — up to 200 hits
  // carrying their verse text — and its key is the query string, so one held per
  // keystroke evicts other panels' answers for fragments nobody will type again.
  const r = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const before = s.cacheSize as number;
    const queries = ["shep", "sheph", "shephe", "shepher", "shepherd", "shepherds"];
    for (const q of queries) await s.fetchQ("searchBlocksScoped", q, "all");
    // One synchronous pass, so no async fill can land between the reads.
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

// Search is a destination screen, not a field sharing the header row with the chapter
// nav and answering into the 380px study sidebar.
test("the glass opens a search screen, not a field in the bar", async ({ page }) => {
  await page.setViewportSize({ width: 412, height: 915 }); // a Pixel's CSS width
  await boot(page);

  await expect(page.locator("header .search")).toHaveCount(0);
  const field = await openSearch(page);

  // The field is the screen's own, and takes most of the width.
  const box = (await field.boundingBox())!;
  expect(box.width / 412).toBeGreaterThan(0.8);

  await field.fill("shepherd");
  await expect(page.getByRole("button", { name: /Psalms 23:1/ }).first()).toBeVisible({ timeout: 30_000 });
  // A result takes the reader to the verse, leaving the screen for the text.
  await page.getByRole("button", { name: /Psalms 23:1/ }).first().click();
  await expect(page.locator(".subtitle")).toHaveText("Psalms 23", { timeout: 30_000 });
});

// Searching a part of the Bible. "shepherd" is in both testaments, so every chip has
// something to drop. Fails against a SearchScreen passing "all" instead of
// `s.searchScope`: the New Testament chip still reports the whole-Bible count.
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

  // The two halves must account for the whole: a scope that quietly widened or dropped
  // verses would not add up.
  expect(ot + nt, `everywhere ${everywhere}, OT ${ot}, NT ${nt}`).toBe(everywhere);
});

// A range answers a question the reader arrived with (the Sermon on the Mount, Paul on
// the law). A span rather than a set of ticks, because those questions are contiguous in
// canon order and a span stays one range test in the engine.
//
// Fails against an `applyRange` that drops the chapters from the token
// (`span:${fromBook}:1:${toBook}:999`): the narrowed count matches the whole book's.
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

// The presets are the canon's own sections (reference::CANON_SEGMENTS), the same rows
// the canon strip paints, so a preset can never name a stretch the strip draws
// differently. Fails against a preset token built from `books[seg.first]` twice:
// "Gospels" returns Matthew's count.
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

// The canon's section names reach the reader translated: the ids in
// `reference::CANON_SEGMENTS` stay English (memory.rs matches them by value) and
// `segment_label` translates what is shown. Checked catalogue → engine → the preset a
// reader taps, driven by catalogue key rather than German words typed here, which would
// be a second copy of the strings under test elsewhere.
//
// Fails against a wire.rs serving `label` straight from CANON_SEGMENTS: the German
// reader is offered "Gospels".
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
    // The canon strip reads the same eight strings, so this pins both surfaces.
    await expect(page.getByRole("button", { name: /Evangelien/ })).toBeVisible({ timeout: 30_000 });
    await expect(page.getByRole("button", { name: /Gesetz/ })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Gospels/ })).toHaveCount(0);
  });
});
