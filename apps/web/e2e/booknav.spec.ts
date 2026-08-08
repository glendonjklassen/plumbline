import { expect, test, type Page } from "@playwright/test";

// The passage navigator, against Android's version (ui/BookNav.kt) — the UX gold
// standard. Three things the web shell was missing (audit 2026-07-29):
//
//   * a testament toggle, so one testament is on screen at a time
//   * a "you are here" marker on the book the pane is already showing
//   * a legend for the reading tint, IN THE DOM. The meaning lived only in each
//     tile's `title`, and a title never fires on touch — so on the platform most
//     readers use, the colours explained nothing.
//
// Selectors are data attributes on purpose. `.grid button` is shared by the book
// grid, the chapter grid and half the dialogs in the shell, and a shared class is
// exactly how a test comes back green about something it never looked at.
//
// And the grid is asserted as ONE SNAPSHOT rather than as a series of
// `toHaveCount(0)`s. A bare "no Genesis tile" is equally satisfied by the dialog
// having closed — which really happened during the mutation runs below, when a dev
// server's hot reload remounted the app mid-assertion and a broken build passed.
// Every check here names what must be present as well as what must not.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  // A fresh profile opens at John 3 — which is what makes the testament default
  // and the "you are here" marker testable without seeding anything.
  await expect(page.locator(".pane .nav button.passage").first()).toHaveText(/John\s+3/);
}

/** Open the navigator the way a reader does — the passage button in the pane. */
async function openNav(page: Page): Promise<void> {
  await page.locator(".pane .nav button.passage").first().click();
  await expect(page.locator(".dialog")).toBeVisible();
}

const ot = (page: Page) => page.locator('.dialog [data-testament="ot"]');
const nt = (page: Page) => page.locator('.dialog [data-testament="nt"]');
const tile = (page: Page, id: string) => page.locator(`.dialog [data-book="${id}"]`);

/** What the book grid is showing right now: which testament is pressed, how many
 *  books are listed, whether two landmark books are among them, and which tiles
 *  claim to be where the reader is. */
async function grid(page: Page) {
  return page.evaluate(() => {
    const ids = [...document.querySelectorAll(".dialog [data-book]")].map((e) =>
      e.getAttribute("data-book"),
    );
    return {
      tab:
        document
          .querySelector('.dialog [data-testament][aria-pressed="true"]')
          ?.getAttribute("data-testament") ?? null,
      count: ids.length,
      hasGen: ids.includes("Gen"),
      hasJohn: ids.includes("John"),
      marked: [...document.querySelectorAll('.dialog [data-book][aria-current="page"]')].map((e) =>
        e.getAttribute("data-book"),
      ),
    };
  });
}

const NT_ONLY = { tab: "nt", count: 27, hasGen: false, hasJohn: true };
const OT_ONLY = { tab: "ot", count: 39, hasGen: true, hasJohn: false };

// Mutation 2026-07-29: `books` in BookNav.svelte changed to `all` — both
// testaments listed at once, the pre-fix behaviour. Failed with
//   Error: the navigator should open on the testament the reader is in
//   expect(received).toEqual(expected) // deep equality
//     Object {
//   -   "count": 27,
//   +   "count": 66,
//   -   "hasGen": false,
//   +   "hasGen": true,
//       "hasJohn": true, "marked": Array ["John"], "tab": "nt" }
test("the testament toggle switches which books are listed, by mouse and by key", async ({ page }) => {
  await boot(page);
  await openNav(page);

  // John 3 is where the reader stands, so the navigator opens in the NT.
  await expect
    .poll(() => grid(page), {
      message: "the navigator should open on the testament the reader is in",
      // The grid paints from the boot-prefetched TOC, which on a cold CI boot
      // may still be a query in flight for a tick or two.
      timeout: 15_000,
    })
    .toEqual({ ...NT_ONLY, marked: ["John"] });
  await expect(tile(page, "Rev")).toBeVisible();

  await ot(page).click();
  await expect
    .poll(() => grid(page), { message: "Old Testament should list the OT, and only the OT" })
    .toEqual({ ...OT_ONLY, marked: [] });
  await expect(tile(page, "Mal")).toBeVisible();

  // Keyboard, because a toggle you can only reach with a thumb is half a toggle.
  await nt(page).focus();
  await page.keyboard.press("Enter");
  await expect
    .poll(() => grid(page), { message: "the toggle should work from the keyboard" })
    .toEqual({ ...NT_ONLY, marked: ["John"] });

  // And the choice survives stepping into a book and back out — from the moment
  // the navigator opens the tab is the reader's, not the pane's.
  await ot(page).click();
  await tile(page, "Gen").click();
  await expect(page.locator(".grid.nums button")).toHaveCount(50);
  await page.locator(".dialog button.crumb").click();
  await expect
    .poll(() => grid(page), { message: "stepping back should return to the tab the reader chose" })
    .toEqual({ ...OT_ONLY, marked: [] });
});

// Mutation 2026-07-29: the `aria-current` binding deleted from the book tile in
// BookNav.svelte. Failed with
//   Error: exactly one tile should say where the reader is
//   expect(received).toEqual(expected) // deep equality
//     Object { "count": 27, "hasGen": false, "hasJohn": true,
//   -   "marked": Array [ "John" ],
//   +   "marked": Array [],
//       "tab": "nt" }
test("the book the reader is in is marked, and only that one", async ({ page }) => {
  await boot(page);
  await openNav(page);

  // Semantics, not a colour: the reading tint already paints every tile, so a
  // test that looked at a background would be satisfied by the map alone.
  await expect
    .poll(() => grid(page), {
      message: "exactly one tile should say where the reader is",
      timeout: 15_000,
    })
    .toEqual({ ...NT_ONLY, marked: ["John"] });

  // The other testament holds nothing the reader is in, so nothing is marked —
  // and the count proves the grid is still there while nothing is marked.
  await ot(page).click();
  await expect
    .poll(() => grid(page), { message: "the Old Testament is not where the reader stands" })
    .toEqual({ ...OT_ONLY, marked: [] });

  // Navigate, and both the marker and the opening tab follow the pane.
  await tile(page, "Gen").click();
  await page.locator(".grid.nums button").first().click();
  await expect(page.locator(".dialog")).toHaveCount(0);
  await expect(page.locator(".pane .nav button.passage").first()).toHaveText(/Genesis\s+1/);
  await openNav(page);
  await expect
    .poll(() => grid(page), { message: "the navigator should follow the reader into the OT" })
    .toEqual({ ...OT_ONLY, marked: ["Gen"] });
});

// THE COLOUR LEGEND IS GONE ON PURPOSE (Glendon, 2026-08-04): "your PWA still
// has the color guide on nav, I don't want that." It was added on 2026-07-29 so
// the tint would explain itself where a `title` never fires, and the test that
// used to live here asserted exactly that. The product call outranks it — a row
// of colour words above the grid is chrome in front of picking a book — so the
// assertion is inverted rather than deleted, to keep it from drifting back in.
//
// What survives from it: the long chapter grid must still scroll to its end.
test("the navigator is the grid, with no colour legend above it", async ({ page }) => {
  await boot(page);
  await openNav(page);

  await expect(page.locator("[data-tint-legend]")).toHaveCount(0);
  // Nor the copy by any other route: no hue words in the dialog's rendered text.
  const shown = await page.locator(".dialog").innerText();
  expect(shown, "the tint copy came back into the navigator").not.toMatch(/not read yet|partway|read through/i);

  // The tiles keep their own explanation, which is where it belongs.
  await ot(page).click();
  await expect(tile(page, "Ps")).toHaveAttribute("title", /.+/);

  await tile(page, "Ps").click();
  await expect(page.locator(".grid.nums button")).toHaveCount(150);
  await page.locator(".dialog .content").evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(page.locator(".grid.nums button").last()).toBeInViewport();
});

// Mark-as-read moved off the first verse's context menu onto the navigator
// (UAT, 2026-08-07): a long-press / right-click on a chapter tile marks it read,
// and a book-level button marks them all. Drives the right-click path (a
// deterministic stand-in for the long-press) and checks the engine actually
// recorded the read.
test("a chapter is marked read from the navigator", async ({ page }) => {
  await boot(page);
  await openNav(page);
  await tile(page, "John").click();
  await expect(page.locator(".grid.nums")).toBeVisible();

  const standingOf = (ch: number) =>
    page.evaluate(async (c) => {
      const r = await (window as any).__plumbline.rpc.call(
        "readingChapters",
        "John",
        new Date().toISOString(),
      );
      return r.chapters.find((x: any) => x.chapter === c)?.standing ?? null;
    }, ch);

  // Not already read — otherwise the assertion below is vacuous.
  expect(await standingOf(5)).not.toBe("read");

  const ch5 = page.locator(".grid.nums button").filter({ hasText: /^5$/ });
  await ch5.click({ button: "right" });
  await expect(page.locator(".tilemenu")).toBeVisible();
  await page.getByRole("menuitem", { name: "Mark read", exact: true }).click();

  await expect.poll(() => standingOf(5), { timeout: 15_000 }).toBe("read");
});
