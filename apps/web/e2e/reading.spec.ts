import { expect, test, type Page } from "@playwright/test";

// The reading map (core::reading): the navigator's tiles must actually change
// with what the reader has read, and the by-hand date must actually land.
//
// These drive the engine through the app's own RPC rather than by reading for
// real, and that is deliberate: crediting a chapter takes as long as reading it
// (words ÷ 220 wpm, after a grace period), so a faithful test would sit for
// minutes per case. What matters — and what a shell can get wrong — is the
// wiring: does a report reach the store, does the store reach the grid, does the
// grid paint it. The arithmetic itself is covered by the 20 unit tests in
// crates/core/src/reading.rs and the ABI round trip in crates/ffi/src/tests.rs,
// where it can be tested at every boundary in microseconds.
//
// Mutation-tested 2026-07-28 (working rules: break the fix, watch it fail):
//   * `tintStyle` returning "" for every heat  → tests 1 and 2 fail (the grid
//     stops carrying the map at all)
//   * `reading` dropped from home.ts USER_DIRS → test 3 fails (the store never
//     reaches IndexedDB, so it does not survive the relaunch)
// Both were run, both went red, both were restored. A test that passes against
// the bug it describes is worse than no test — and the first cut of
// `waitForPersist` here was exactly that: `page.waitForFunction` with an async
// predicate returned truthy on its first tick, so the helper "waited" for
// nothing and the reload beat the 50 ms persist debounce. It uses expect.poll.

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
 * Wait out the background pipeline before measuring anything about the cache.
 *
 * `onWarmReady` and `onRndReady` both call the whole-cache `invalidate()`, and in
 * a fresh test profile they land within a few seconds — which MASKS a stale
 * reading map completely. The first cut of the staleness test below passed with
 * its own fix reverted for exactly this reason: the warm was quietly refetching
 * everything underneath it.
 *
 * On a real device the warm settles early in a session and then never fires
 * again, so a chapter finished twenty minutes later has nothing to refresh it.
 * Settling first is what makes the test live in that world instead of in the
 * first five seconds of one.
 */
async function settleBackground(page: Page): Promise<void> {
  // NOT gated on `rndState === "ready"`. The analysis tiers are opt-in since
  // v0.32.0, so on a fresh profile the R&D pack is never fetched and that state
  // never arrives — waiting for it just times out. What this actually needs is
  // "no further invalidation is coming", and a boot trace that has stopped
  // growing says exactly that: every warm and analysis step appends one entry.
  await page.waitForFunction(
    async () => {
      const n = ((await (window as any).__plumbline.rpc.bootTrace()) ?? []).length;
      const prev = (window as any).__settleLen ?? -1;
      (window as any).__settleLen = n;
      return n === prev && n > 10;
    },
    null,
    { timeout: 120_000, polling: 1500 },
  );
}

/** Open Go to… and step into a book, so the chapter grid is on screen. */
async function openChapterGrid(page: Page, bookName: string): Promise<void> {
  await page.evaluate(() => {
    (window as any).__plumbline.bookNavFor = 0;
  });
  // The navigator lists ONE testament, opening on the one the reader is standing
  // in (Android's pattern, ported 2026-07-29) — and a fresh test profile is at
  // John 3, i.e. the New Testament. So ask for the testament the book is in
  // before looking for the book; the tab is a no-op if it is already current.
  const otBooks = new Set(["Genesis", "Exodus", "Psalms", "Isaiah", "Malachi"]);
  await page.locator(`.dialog [data-testament="${otBooks.has(bookName) ? "ot" : "nt"}"]`).click();
  const book = page.getByRole("button", { name: bookName, exact: true });
  await expect(book).toBeVisible({ timeout: 15_000 });
  await book.click();
  await expect(page.locator(".grid.nums button").first()).toBeVisible();
}

/** The inline style Svelte put on chapter tile `n` (1-based). */
function chapterTile(page: Page, n: number) {
  return page.locator(".grid.nums button").nth(n - 1);
}

// The three hues, as core::theme's LIGHT palette emits them (readUnread #c9a227,
// readPartial #a8642c, readDone #6f8f6a). Asserting the actual rgb is the point:
// "has a background" is true of every tile — an unread chapter is deliberately
// tinted, not left blank — so only the hue distinguishes the states.
const GOLD = "201, 162, 39";
const SAGE = "111, 143, 106";

/** Assert a tile's hue, waiting for it.
 *
 *  The grids paint SYNCHRONOUSLY from the TOC and the reading tint arrives a beat
 *  later through the read-through cache — that is the design (the chapter numbers
 *  must never wait on a query). So the hue needs a retrying assertion; a
 *  `getAttribute` snapshot races the fill and reads "". */
async function expectHue(page: Page, n: number, rgb: string): Promise<void> {
  await expect(chapterTile(page, n)).toHaveAttribute("style", new RegExp(rgb.replace(/ /g, "\\s*")));
}

/** Wait until the reading store has actually reached IndexedDB.
 *
 *  Authoring writes persist on a 50 ms debounce inside the worker, so a reload
 *  fired immediately after can outrun them. Polling the real store (rather than
 *  sleeping) keeps this a test about durability instead of about timing.
 *
 *  `expect.poll`, NOT `page.waitForFunction`: an async predicate handed to
 *  waitForFunction resolved truthy on its first tick here and the helper returned
 *  before the keys existed at all — which made a green helper hide a red test.
 *  expect.poll awaits the promise it is given. */
async function waitForPersist(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(async () => {
          const db = await new Promise<IDBDatabase>((res, rej) => {
            const r = indexedDB.open("plumbline", 1);
            r.onsuccess = () => res(r.result);
            r.onerror = () => rej(r.error);
          });
          const keys = await new Promise<IDBValidKey[]>((res) => {
            const q = db.transaction("user").objectStore("user").getAllKeys();
            q.onsuccess = () => res(q.result);
            q.onerror = () => res([]);
          });
          db.close();
          return keys.some((k) => String(k).startsWith("reading/"));
        }),
      { timeout: 20_000, message: "the reading store should reach IndexedDB" },
    )
    .toBe(true);
}

test("an unread canon invites, and a chapter marked read is tinted and titled", async ({ page }) => {
  await boot(page);

  // ── a fresh profile has read nothing: gold, and lit from the first launch ──
  // Not calm — INVITING. A map whose job is to show a reader where to go must not
  // start dark (2026-07-29), so unread glows at once rather than ramping.
  await openChapterGrid(page, "Genesis");
  await expectHue(page, 1, GOLD);
  const before = await chapterTile(page, 1).getAttribute("style");
  expect(before, "unread must invite immediately").toContain("box-shadow");
  await expect(chapterTile(page, 1)).toHaveAttribute("title", /not read yet/);

  // ── log Genesis 1 by hand, a year and a bit ago ──
  const stale = new Date();
  stale.setUTCFullYear(stale.getUTCFullYear() - 2);
  const staleDay = stale.toISOString().slice(0, 10);
  const err = await page.evaluate(
    (d) => (window as any).__plumbline.author("readingMarkRead", "Gen", 1, d),
    staleDay,
  );
  expect(err, "markRead should succeed").toBeNull();

  // Re-open so the grid re-queries (the map is fetched per navigator open).
  await page.evaluate(() => {
    (window as any).__plumbline.bookNavFor = null;
  });
  await openChapterGrid(page, "Genesis");

  // Read through, and two years stale: the hue flips to sage AND it blooms.
  await expect(chapterTile(page, 1)).toHaveAttribute("title", /read through, last .* years? ago/);
  await expectHue(page, 1, SAGE);
  const after = await chapterTile(page, 1).getAttribute("style");
  expect(after, "two years untouched must bloom").toContain("box-shadow");

  // Its neighbour is untouched — the map is per chapter, not per book — and it is
  // the READ one that is now quiet-by-recency while the unread one still calls.
  await expectHue(page, 2, GOLD);
  await expect(chapterTile(page, 2)).toHaveAttribute("title", /not read yet/);
});

test("reading time is credited, and only with both scroll and dwell", async ({ page }) => {
  await boot(page);

  // Scroll to the end with no time at all: the shape of flipping through a book.
  // It must credit NOTHING — this is the guard the whole design turns on.
  const flipped = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return s.rpc.call("readingRecord", "Gen", 2, 25, 0, new Date().toISOString());
  });
  expect(flipped.pct, "scrolling with no dwell must credit nothing").toBe(0);
  expect(flipped.completed).toBe(false);

  // Ample time, but never past verse 1: credited only for what was on screen.
  const parked = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return s.rpc.call("readingRecord", "Gen", 3, 1, 3600, new Date().toISOString());
  });
  expect(parked.completed, "an hour on verse 1 is not a read chapter").toBe(false);
  expect(parked.pct).toBeGreaterThan(0);
  expect(parked.pct).toBeLessThan(0.2);

  // Both: through the chapter, with the time it takes. That is a read.
  const read = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return s.rpc.call("readingRecord", "Gen", 4, 999, 3600, new Date().toISOString());
  });
  expect(read.completed, "scrolled through with ample dwell must complete").toBe(true);
  expect(read.pct).toBe(1);

  // And it shows up in the grid, sage and quiet (read today ⇒ no bloom).
  await openChapterGrid(page, "Genesis");
  await expect(chapterTile(page, 4)).toHaveAttribute("title", /read through, last today/);
  await expectHue(page, 4, SAGE);
  const style = await chapterTile(page, 4).getAttribute("style");
  expect(style, "read today must not bloom").not.toContain("box-shadow");
});

test("finishing a chapter updates the map without a relaunch", async ({ page }) => {
  await boot(page);
  // Nothing else may be invalidating while this is measured — see settleBackground.
  await settleBackground(page);

  // The navigator asks for the map through the read-through cache, keyed per DAY
  // so a fresh key is not minted every second. That is right, and on its own it
  // meant a chapter finished mid-session did not appear until the next launch
  // ("when I read a book, and flip to the next chapter, the heatmap doesn't update
  // until next app load", 2026-07-29): the dwell report deliberately skips the
  // `authored` event, and skipped the cache invalidation with it.
  //
  // NOTE ON WHAT THIS CAN AND CANNOT CATCH. The background warm calls the
  // whole-cache `invalidate()` when it finishes, so on a fresh profile it may
  // refresh the reading map as a side effect and let a broken build pass here.
  // Settling first makes that unlikely but not impossible — it is a race, and a
  // race is not a guard. The mutation-proof assertion for this fix is
  // `invalidateOnly drops only what it is told to` below; this test is the
  // end-to-end path, and it will still catch the fix disappearing entirely.
  const cachedGen1 = () =>
    page.evaluate(() => {
      const s = (window as any).__plumbline;
      const stamp = new Date().toISOString().slice(0, 10) + "T12:00:00Z";
      const r = s.q("readingChapters", "Gen", stamp);
      return r?.chapters?.find((c: any) => c.chapter === 1) ?? null;
    });

  // Prime it exactly as opening the navigator does, and let the fetch land.
  await openChapterGrid(page, "Genesis");
  await expect(chapterTile(page, 1)).toHaveAttribute("title", /not read yet/);
  await page.evaluate(() => ((window as any).__plumbline.bookNavFor = null));
  expect((await cachedGen1())?.standing, "primed as unread").toBe("unread");

  // Read it through, exactly as the tracker reports it.
  const done = await page.evaluate(() =>
    (window as any).__plumbline.rpc.call("readingRecord", "Gen", 1, 999, 3600, new Date().toISOString()),
  );
  expect(done.completed, "the chapter should have completed").toBe(true);

  // The stale answer must be GONE — dropped, then refetched as read. Without the
  // invalidation it sits there saying "unread" until the next launch.
  await expect
    .poll(async () => (await cachedGen1())?.standing, {
      timeout: 15_000,
      message: "the map should know the chapter was finished, in this session",
    })
    .toBe("read");

  // And the navigator paints it.
  await openChapterGrid(page, "Genesis");
  await expect(chapterTile(page, 1)).toHaveAttribute("title", /read through, last today/);
  await expectHue(page, 1, SAGE);
});

test("invalidateOnly drops only what it is told to", async ({ page }) => {
  await boot(page);

  // The mechanism behind the fix, tested where nothing else can interfere. A
  // dwell report lands every 30 seconds while somebody reads, so it must drop the
  // reading map WITHOUT throwing away the word study they have open or every
  // thread and tag read on screen — which is what the whole-cache `invalidate()`
  // would do, and why this method exists.
  const state = () =>
    page.evaluate(() => {
      const s = (window as any).__plumbline;
      const stamp = new Date().toISOString().slice(0, 10) + "T12:00:00Z";
      return {
        reading: s.q("readingChapters", "Gen", stamp) !== null,
        threads: s.q("threads") !== null,
      };
    });

  // Prime both.
  await expect.poll(async () => (await state()).reading, { timeout: 20_000 }).toBe(true);
  await expect.poll(async () => (await state()).threads, { timeout: 20_000 }).toBe(true);

  // Drop the reading reads only.
  const after = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.invalidateOnly("readingBooks", "readingChapters");
    const stamp = new Date().toISOString().slice(0, 10) + "T12:00:00Z";
    // Read back synchronously, before any refetch can land.
    return {
      reading: s.q("readingChapters", "Gen", stamp) !== null,
      threads: s.q("threads") !== null,
    };
  });
  expect(after.reading, "the reading map is dropped and will refetch").toBe(false);
  expect(after.threads, "everything else is left alone").toBe(true);
});

test("the reading map survives a relaunch and rides in the backup", async ({ page }) => {
  await boot(page);

  const day = "2026-03-09";
  const markErr = await page.evaluate(
    (d) => (window as any).__plumbline.author("readingMarkRead", "Exod", 3, d),
    day,
  );
  expect(markErr, "markRead should succeed").toBeNull();

  // A relaunch: the store is on the device, not in the tab.
  await waitForPersist(page);
  await page.reload();
  await boot(page);
  const chapters = await page.evaluate(() =>
    (window as any).__plumbline.rpc.call("readingChapters", "Exod", new Date().toISOString()),
  );
  const ch3 = chapters.chapters.find((c: any) => c.chapter === 3);
  expect(ch3.lastRead, "a logged read must outlive the tab").toBe(day);
  expect(ch3.standing).toBe("read");

  // The backup carries it, or it is not really the reader's data.
  const paths = await page.evaluate(async () =>
    [...(await (window as any).__plumbline.rpc.exportUserData())].map(([p]: [string]) => p),
  );
  expect(paths.some((p: string) => p.startsWith("reading/"))).toBe(true);
});

// Tapping a verse must land ON the verse, not at the top of its chapter — a UAT
// bug report (2026-08-06). The web funnels every verse-level tap (search goto,
// cross-refs, the reading map, notes) through `Session.navigate(pane, book,
// chapter, verse)`, so this drives that one function and watches the viewport.
// It reproduces the drop directly: if the verse is ignored, the pane sits at the
// top and scrollTop stays ~0. Ps 119 is the KJV's longest chapter (176 verses),
// bundled and frozen, so a late verse is unmistakably far down the page.
test("navigating to a verse scrolls it into view, not just the chapter", async ({ page }) => {
  await boot(page);
  const scrollTop = () =>
    page.locator(".pane .scroll").first().evaluate((el) => (el as HTMLElement).scrollTop);

  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Ps", 119, 170));
  await expect
    .poll(() => page.evaluate(() => (window as any).__plumbline.panes[0].targetVerse))
    .toBe(170);
  // The one that reproduces the bug: a dropped verse leaves this at the top.
  await expect.poll(scrollTop, "a late verse must scroll the pane down").toBeGreaterThan(400);

  // And the offset is the VERSE's, not a constant: verse 1 of the same long
  // chapter sits at the top. (A fix that always scrolled a fixed amount would
  // pass the check above and fail this one.)
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Ps", 119, 1));
  await expect.poll(scrollTop, "verse 1 sits at the top of the chapter").toBeLessThan(40);
});
