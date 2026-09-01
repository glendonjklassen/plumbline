import { expect, test, type Page } from "@playwright/test";

// The reading map (core::reading): the navigator's tiles change with what the reader has read, and
// a by-hand date lands. These drive the engine through the app's own RPC rather than reading for
// real — crediting a chapter takes as long as reading it (words ÷ 220 wpm, after a grace period) —
// so what is tested here is the wiring, not the arithmetic, which is covered by unit tests in
// crates/core/src/reading.rs and the ABI round trip in crates/ffi/src/tests.rs.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/**
 * Wait out the background pipeline before measuring anything about the cache.
 *
 * `onWarmReady` and `onRndReady` both call the whole-cache `invalidate()`, and on a fresh profile
 * they land within seconds, masking a stale reading map completely. On a real device the warm
 * settles early in a session and never fires again, so settling first puts the test in that world.
 */
async function settleBackground(page: Page): Promise<void> {
  // Not gated on `rndState === "ready"`: the analysis tiers are opt-in, so on a fresh profile the
  // R&D pack is never fetched and that state never arrives. The real signal is "no further
  // invalidation is coming", and every warm and analysis step appends one boot-trace entry.
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
  // The navigator lists one testament, opening on the one the reader is in, and a fresh profile is
  // at John 3. Ask for the book's testament first; the tab is a no-op if it is already current.
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

// The hues core::theme's LIGHT palette emits (readUnread #c9a227, readPartial #a8642c, readDone
// #6f8f6a). The rgb must be asserted: every tile has a background — unread is deliberately tinted,
// not left blank — so only the hue distinguishes the states.
const GOLD = "201, 162, 39";
const SAGE = "111, 143, 106";

/** Assert a tile's hue, waiting for it. The grid paints synchronously from the TOC and the tint
 *  arrives a beat later through the read-through cache, so a `getAttribute` snapshot races the
 *  fill and reads "". */
async function expectHue(page: Page, n: number, rgb: string): Promise<void> {
  await expect(chapterTile(page, n)).toHaveAttribute("style", new RegExp(rgb.replace(/ /g, "\\s*")));
}

/** Wait until the reading store has actually reached IndexedDB.
 *
 *  Authoring writes persist on a 50 ms debounce inside the worker, so a reload fired immediately
 *  after can outrun them. `expect.poll`, not `page.waitForFunction`: an async predicate handed to
 *  waitForFunction resolves truthy on its first tick, returning before the keys exist at all. */
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

  // A fresh profile has read nothing: gold, and lit from the first launch. A map that shows a
  // reader where to go must not start dark, so unread glows at once rather than ramping up.
  await openChapterGrid(page, "Genesis");
  await expectHue(page, 1, GOLD);
  const before = await chapterTile(page, 1).getAttribute("style");
  expect(before, "unread must invite immediately").toContain("box-shadow");
  await expect(chapterTile(page, 1)).toHaveAttribute("title", /not read yet/);

  // Log Genesis 1 by hand, two years back.
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

  // Read through and two years stale: the hue flips to sage and it blooms.
  await expect(chapterTile(page, 1)).toHaveAttribute("title", /read through, last .* years? ago/);
  await expectHue(page, 1, SAGE);
  const after = await chapterTile(page, 1).getAttribute("style");
  expect(after, "two years untouched must bloom").toContain("box-shadow");

  // Its neighbour is untouched: the map is per chapter, not per book.
  await expectHue(page, 2, GOLD);
  await expect(chapterTile(page, 2)).toHaveAttribute("title", /not read yet/);
});

test("reading time is credited, and only with both scroll and dwell", async ({ page }) => {
  await boot(page);

  // Flipping through a book: scrolled to the end with no dwell at all.
  const flipped = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return s.rpc.call("readingRecord", "Gen", 2, 25, 0, new Date().toISOString());
  });
  expect(flipped.pct, "scrolling with no dwell must credit nothing").toBe(0);
  expect(flipped.completed).toBe(false);

  // Ample dwell but never past verse 1: credit follows what was on screen.
  const parked = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return s.rpc.call("readingRecord", "Gen", 3, 1, 3600, new Date().toISOString());
  });
  expect(parked.completed, "an hour on verse 1 is not a read chapter").toBe(false);
  expect(parked.pct).toBeGreaterThan(0);
  expect(parked.pct).toBeLessThan(0.2);

  // Both: through the chapter, with the time it takes.
  const read = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return s.rpc.call("readingRecord", "Gen", 4, 999, 3600, new Date().toISOString());
  });
  expect(read.completed, "scrolled through with ample dwell must complete").toBe(true);
  expect(read.pct).toBe(1);

  // It shows in the grid, sage and quiet: read today means no bloom.
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

  // The bug: the map is read through a cache keyed per day, and the dwell report deliberately
  // skips the `authored` event — skipping the cache invalidation with it — so a chapter finished
  // mid-session did not appear until the next launch.
  //
  // What this can and cannot catch: the background warm's whole-cache `invalidate()` could refresh
  // the map as a side effect and let a broken build pass. Settling first makes that unlikely, not
  // impossible; the assertion that cannot race is `invalidateOnly drops only what it is told to`
  // below. This one covers the end-to-end path and still catches the fix disappearing outright.
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

  // The stale answer must be dropped and refetched as read; without the invalidation it stays
  // "unread" until the next launch.
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

  // A dwell report lands every 30 seconds while somebody reads, so it must drop the reading map
  // without throwing away the open word study or the threads and tags on screen — which is what
  // the whole-cache `invalidate()` would do.
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

  // The backup must carry it.
  const paths = await page.evaluate(async () =>
    [...(await (window as any).__plumbline.rpc.exportUserData())].map(([p]: [string]) => p),
  );
  expect(paths.some((p: string) => p.startsWith("reading/"))).toBe(true);
});

// The bug: a verse-level tap landed at the top of the chapter, dropping the verse. Every such tap
// (search goto, cross-refs, the reading map, notes) goes through `Session.navigate(pane, book,
// chapter, verse)`, so this drives that and watches the viewport — a dropped verse leaves
// scrollTop at ~0. Ps 119 is the KJV's longest chapter, so a late verse is far down the page.
test("navigating to a verse scrolls it into view, not just the chapter", async ({ page }) => {
  await boot(page);
  const scrollTop = () =>
    page.locator(".pane .scroll").first().evaluate((el) => (el as HTMLElement).scrollTop);

  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Ps", 119, 170));
  await expect
    .poll(() => page.evaluate(() => (window as any).__plumbline.panes[0].targetVerse))
    .toBe(170);
  await expect.poll(scrollTop, "a late verse must scroll the pane down").toBeGreaterThan(400);

  // The offset is the verse's, not a constant: a fix that always scrolled a fixed amount would
  // pass the check above and fail this one.
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Ps", 119, 1));
  await expect.poll(scrollTop, "verse 1 sits at the top of the chapter").toBeLessThan(40);
});
