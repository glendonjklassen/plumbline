import { expect, test, type Page } from "@playwright/test";

// Every chapter in the Bible used to live at `/`.
//
// Nothing was bookmarkable (audit item D-05): a reader who wanted to send someone
// a passage had to go through the verse-share menu, a reader who wanted to come
// back to one had to navigate there again, and on an installed PWA the phone's
// Back button had nothing under the reader but the launch — so Back out of an open
// study sheet left the app.
//
// Four properties, and they are the four ways this can be got wrong:
//
//   1. the address FOLLOWS the reader (`#/John/3`, the OSIS book id) and a chapter
//      turn REPLACES the entry — forty verses into Psalms, a pushing router would
//      cost forty Back presses to leave;
//   2. an incoming address BEATS the restored position, which is the whole point
//      of a link somebody sent;
//   3. Back closes what is on top of the text instead of exiting the app, which is
//      what Android has done since it shipped (BackHandler);
//   4. an address we cannot read falls back to the reader, never to a blank page.
//
// (2) and (4) are the pair that has to be tested together: a router that lands
// nowhere on a good address fails (2), and one that trusts any address fails (4)
// by opening a pane on a chapter that does not exist.
//
// WHY `about:blank` BETWEEN BOOTS. Two URLs that differ only in their fragment are
// the SAME DOCUMENT to a browser, so `page.goto("/#/Rom/8")` from `/#/Ps/23` does
// not reload anything — it fires `hashchange` and returns. A test written without
// this would still pass while proving the wrong mechanism: the live router
// answering a fragment change, rather than the app booting at an address. So every
// re-entry leaves the origin first, and what follows is a real cold start.
//
// MUTATION RECIPES — NOT YET RUN. This agent could not run Playwright (shared
// dist/ and preview port), so nothing below is claimed to pass. Each recipe names
// the file, the edit, and the test title that must go red:
//
//   1. apps/web/src/App.svelte — delete the body of the `$effect(() => {
//      session?.syncUrl(); })` block. → "the address bar follows the reader from
//      chapter to chapter" red at `#/John/6` (installRouter still stamps the first
//      address, so the failure is on the TURN, which is the half that matters).
//   2. apps/web/src/state/session.svelte.ts — in `syncUrl`, `history.replaceState`
//      → `history.pushState`. → same test red on "three chapter turns must not
//      cost three Back presses" (3 entries piled up instead of 0).
//   3. apps/web/src/state/session.svelte.ts — in `syncUrl`, pass the display name:
//      `Session.hashRoute(this.bookName(pane.book), pane.chapter)`. → same test red
//      on `#/1John/3` (received `#/1%20John/3`).
//   4. apps/web/src/App.svelte — delete the `const routed = …; if (routed)
//      s.navigate(…)` pair. → "an address opens where it points, over the position
//      the reader left" red on Rom 8 (received Ps 23, the restored session).
//   5. apps/web/src/state/session.svelte.ts — in `routeFromHash`, delete the
//      display-name fallback (`?? books.find((b) => String(b.name)…)`). → same
//      test red on `#/Genesis/50` (received Rom 8 — the step before it, which by
//      then is the persisted session position).
//   6. apps/web/src/App.svelte — in the surface effect, drop the push: `if
//      (s.transientOpen) { /* nothing */ } else s.dropSurfaceEntry();`. → "the
//      phone Back button closes an open surface instead of leaving the app" red on
//      "opening a surface must push exactly one history entry", and then on
//      "Back left the document" — with no entry to spend, Back leaves the origin.
//   7. apps/web/src/state/session.svelte.ts — in `installRouter`'s popstate
//      handler, delete `this.dismissTransient();`. → same test red on "Back did not
//      close the surface".
//   8. apps/web/src/state/session.svelte.ts — in `routeFromHash`, weaken the range
//      check to `if (chapter < 1) return null;`. → "an address the app cannot read
//      still boots to a readable chapter" red on `#/Ps/151` (navigate clamps, so
//      the reader lands in Psalm 150 instead of falling back). Dropping the other
//      half (`chapter < 1`) instead turns `#/John/0` red, for John 1.
//   9. apps/web/src/state/session.svelte.ts — in `routeFromHash`, remove the
//      try/catch around `decodeURIComponent`. → same test red on `#/%E0%A4%A`: the
//      URIError escapes into `start()`, which paints the splash error, so the
//      reader canvas never appears.
//  10. apps/web/src/App.svelte — `if (at)` → `if (at && !routed)`. → "a shared
//      verse still wins over an address left in the same link" red (Rom 8 instead
//      of Isa 53:5). This is the `?at=` plumbing the two have to coexist with
//      (share-verse.spec.ts).
//  11. apps/web/src/App.svelte — drop the arrivals-only guard: `const routed =
//      s.routeFromHash(location.hash);`. → nothing HERE goes red (every boot in
//      this file is a real navigation), but e2e/legacy-restore.spec.ts goes red on
//      "the restored session opened somewhere else entirely": a restore reloads,
//      and the address left over from the replaced session would outrank the
//      backup's own last position. That existing test is this rule's guard, which
//      is why this file does not duplicate it.

/** The lighter of the two boot helpers in this suite (share-verse.spec.ts's, not
 *  app.spec.ts's): routing has nothing to do with the analysis tiers, so the
 *  first-run checkboxes are left alone and no pack is downloaded for them. */
async function boot(page: Page, url = "/"): Promise<void> {
  await page.goto(url);
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Boot at `url` from OUTSIDE the origin, so a fragment-only difference from the
 *  address already showing cannot turn a cold start into a same-document
 *  `hashchange` (see the note at the top of this file). */
async function reboot(page: Page, url: string): Promise<void> {
  await page.goto("about:blank");
  await boot(page, url);
}

/** Where pane 0 is pointing. Pane 0 and not the active pane: it is the one the
 *  address mirrors, and on a phone it is the only one there is. */
async function where(page: Page): Promise<{ book: string; chapter: number; verse: number | null }> {
  return await page.evaluate(() => {
    const p = (window as any).__plumbline.panes[0];
    return { book: p.book, chapter: p.chapter, verse: p.targetVerse };
  });
}

const hashOf = (page: Page): string => new URL(page.url()).hash;

const historyLength = (page: Page): Promise<number> => page.evaluate(() => history.length);

test("the address bar follows the reader from chapter to chapter", async ({ page }) => {
  await boot(page);
  await expect
    .poll(() => hashOf(page), { message: "the chapter the app opened in has no address", timeout: 20_000 })
    .toBe("#/John/3");

  // Three chapter turns by the key a reader actually presses (Shell's `]`), so a
  // hash that only tracks `navigate` is not enough to pass.
  const before = await historyLength(page);
  for (let i = 0; i < 3; i++) await page.keyboard.press("]");
  await expect
    .poll(() => where(page), { message: "the ] key never turned the page", timeout: 20_000 })
    .toMatchObject({ book: "John", chapter: 6 });
  await expect
    .poll(() => hashOf(page), {
      message: "the address stayed behind while the reader moved",
      timeout: 10_000,
    })
    .toBe("#/John/6");

  // THE property that makes this replaceState and not pushState.
  expect(
    (await historyLength(page)) - before,
    "three chapter turns must not cost three Back presses to leave the app",
  ).toBe(0);

  // The book travels as its OSIS ID. A display name would put a space in the
  // address ("1 John" → `#/1%20John/3`) and stop being the frozen wire form.
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "1John", 3));
  await expect
    .poll(() => hashOf(page), { message: "a numbered book's address is not its OSIS id", timeout: 20_000 })
    .toBe("#/1John/3");
});

test("an address opens where it points, over the position the reader left", async ({ page }) => {
  await boot(page);
  // Leave the session somewhere the default boot would never land, and make sure
  // it really reached the disk — `flushConfig` posts to the worker, which debounces
  // its persist, and the RPC is ordered so the flush carries the config with it.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.navigate(0, "Ps", 23);
    s.flushConfig();
    await s.rpc.flush();
  });

  // The PREMISE, asserted rather than assumed: without it, "the address won" could
  // be nothing more than the app landing where it always lands.
  await reboot(page, "/");
  await expect
    .poll(() => where(page), { message: "the session did not restore where it was left", timeout: 20_000 })
    .toMatchObject({ book: "Ps", chapter: 23 });

  // The canonical form, and the one a share hands over.
  await reboot(page, "/#/Rom/8");
  await expect
    .poll(() => where(page), {
      message: "a bookmarked address lost to the restored session",
      timeout: 20_000,
    })
    .toMatchObject({ book: "Rom", chapter: 8 });

  // Liberal on the way in: a display name is what somebody types or dictates.
  //
  // EVERY address below names a different chapter from the one before it, and that
  // is load-bearing: each step's arrival is persisted a moment after it lands
  // (`saveConfig` debounces 300 ms, and every step here takes seconds), so each
  // step becomes the next step's restore point. Re-using `#/Rom/8` here would have
  // passed with the whole display-name branch deleted.
  await reboot(page, "/#/Genesis/50");
  await expect
    .poll(() => where(page), { message: "a hand-typed book name opened nothing", timeout: 20_000 })
    .toMatchObject({ book: "Gen", chapter: 50 });

  // Spaces and case too — and the address it settles at is the canonical one.
  await reboot(page, "/#/1%20John/3");
  await expect
    .poll(() => where(page), { message: "`#/1 John/3` opened nothing", timeout: 20_000 })
    .toMatchObject({ book: "1John", chapter: 3 });
  expect(hashOf(page), "the app should settle at the canonical address").toBe("#/1John/3");
});

test("the phone Back button closes an open surface instead of leaving the app", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 780 });
  await boot(page);
  await expect.poll(() => hashOf(page), { timeout: 20_000 }).toBe("#/John/3");

  const before = await historyLength(page);
  // Driven through session state, as e2e/surfaces.spec.ts drives every surface:
  // this test is about the history entry, not about how a sheet gets opened.
  await page.evaluate(() => ((window as any).__plumbline.panel = { kind: "guide" }));
  await expect(page.locator('[data-surface="study panel"]')).toBeVisible({ timeout: 15_000 });
  await expect
    .poll(() => historyLength(page).then((n) => n - before), {
      message: "opening a surface must push exactly one history entry",
      timeout: 10_000,
    })
    .toBe(1);

  // A marker that survives only if the document does. Without it this test passes
  // vacuously on the very bug it describes: leaving the app also "closes" the
  // sheet, and a fresh boot would report a closed panel just as happily.
  await page.evaluate(() => ((window as any).__stillHere = "yes"));
  // The assertions are below, not here: a same-document traversal resolves with no
  // response, and what matters is what the app did rather than what the navigation
  // promise said about it.
  await page.goBack({ timeout: 15_000 }).catch(() => {});

  expect(
    await page.evaluate(() => (window as any).__stillHere ?? "the app was left"),
    "Back left the document — the surface had no history entry of its own",
  ).toBe("yes");
  await expect
    .poll(
      () =>
        // Reported through the session object and not `?? null`, which would turn
        // a missing app into the very null this asserts.
        page.evaluate(() => {
          const s = (window as any).__plumbline;
          return s ? s.panel : "no app";
        }),
      { message: "Back did not close the surface", timeout: 10_000 },
    )
    .toBeNull();
  await expect(page.locator('[data-surface="study panel"]')).toHaveCount(0);
  // Still in the same chapter, at its own address: closing a sheet is not a
  // navigation, and the entry Back landed on must say where the reader IS.
  await expect(page.locator(".pane canvas").first()).toBeVisible();
  expect(hashOf(page)).toBe("#/John/3");
  expect(await where(page)).toMatchObject({ book: "John", chapter: 3 });
});

test("an address the app cannot read still boots to a readable chapter", async ({ page }) => {
  // Every shape of junk that can reach us: a book nobody has, a chapter before the
  // first, a chapter past the last, something that is not a route at all, and an
  // escape sequence that makes decodeURIComponent throw.
  //
  // `#/Ps/151` and not `#/Ps/9999`: four digits fail the ROUTE SHAPE, so they would
  // never reach the range check and a test using them could not tell whether the
  // check works. 151 is a well-formed address for a psalm that does not exist.
  const JUNK = ["#/Nowhere/3", "#/John/0", "#/Ps/151", "#notaroute", "#/%E0%A4%A"];

  await boot(page); // once, to get past first-run and warm the depot
  for (const junk of JUNK) {
    await reboot(page, `/${junk}`);
    // `boot` has already waited for the reader canvas, which is the "never a blank
    // page" half. This is the "and it is the RIGHT chapter" half.
    await expect
      .poll(() => where(page), { message: `${junk} did not fall back to the reader`, timeout: 20_000 })
      .toMatchObject({ book: "John", chapter: 3 });
    // And the address is repaired rather than left lying: whatever the reader
    // bookmarks next must say where they actually are.
    await expect
      .poll(() => hashOf(page), { message: `${junk} was left in the address bar`, timeout: 20_000 })
      .toBe("#/John/3");
  }
});

test("a shared verse still wins over an address left in the same link", async ({ page }) => {
  // `?at=` (church.ts, share-verse.spec.ts) names a VERSE, so it is the more
  // specific of the two and must win — a hash in the same link is at best the
  // chapter the sender happened to be looking at.
  await boot(page, "/?at=Isa%2053%3A5#/Rom/8");
  await expect
    .poll(() => where(page), { message: "the hash overrode the shared verse", timeout: 20_000 })
    .toEqual({ book: "Isa", chapter: 53, verse: 5 });
  // The link's payload is still stripped from the address bar (App.svelte), and
  // what is left is the bookmarkable form of where the reader ended up.
  const landed = new URL(page.url());
  expect(landed.search, "a bookmark must not be a link about somebody's church").toBe("");
  expect(landed.hash).toBe("#/Isa/53");
});
