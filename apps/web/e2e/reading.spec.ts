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

/** Open Go to… and step into a book, so the chapter grid is on screen. */
async function openChapterGrid(page: Page, bookName: string): Promise<void> {
  await page.evaluate(() => {
    (window as any).__plumbline.bookNavFor = 0;
  });
  const book = page.getByRole("button", { name: bookName, exact: true });
  await expect(book).toBeVisible({ timeout: 15_000 });
  await book.click();
  await expect(page.locator(".grid.nums button").first()).toBeVisible();
}

/** The inline style Svelte put on chapter tile `n` (1-based). */
function chapterTile(page: Page, n: number) {
  return page.locator(".grid.nums button").nth(n - 1);
}

// The three hues, as core::theme's LIGHT palette emits them (readUnread #6b7a8f,
// readPartial #c98a2e, readDone #6f8f6a). Asserting the actual rgb is the point:
// "has a background" is true of every tile — an unread chapter is deliberately
// tinted slate, not left blank — so only the hue distinguishes the states.
const SLATE = "107, 122, 143";
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

test("an unread canon is calm, and a chapter marked read is tinted and titled", async ({ page }) => {
  await boot(page);

  // ── a fresh profile has read nothing: slate everywhere, and no bloom ──
  // Calm is the promise, not blank: a brand-new install must not shout, so every
  // tile is tinted the "not started" hue with no glow on top of it.
  await openChapterGrid(page, "Genesis");
  await expectHue(page, 1, SLATE);
  const before = await chapterTile(page, 1).getAttribute("style");
  expect(before, "a fresh install must not bloom").not.toContain("box-shadow");
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

  // Its neighbour is untouched — the map is per chapter, not per book.
  await expectHue(page, 2, SLATE);
  const neighbour = await chapterTile(page, 2).getAttribute("style");
  expect(neighbour, "an unread neighbour stays quiet").not.toContain("box-shadow");
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
