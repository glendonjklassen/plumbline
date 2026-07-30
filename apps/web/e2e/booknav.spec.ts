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

// Mutation 2026-07-29: the `<p class="legend" data-tint-legend>` element removed
// from BookNav.svelte — the pre-fix state, where `title` was the only
// explanation. Failed with
//   Error: expect(locator).toBeVisible() failed
//   Locator: locator('[data-tint-legend]')
//   Expected: visible
//   Error: element(s) not found
test("the reading tint explains itself on screen, not in a tooltip", async ({ page }) => {
  await boot(page);
  await openNav(page);

  const legend = page.locator("[data-tint-legend]");
  await expect(legend).toBeVisible();

  // `innerText` is the assertion that matters: it is the RENDERED text, so a
  // `title` — the thing that never fires on touch — contributes nothing to it.
  // The tiles still carry their own titles, and this must pass without them.
  const shown = await legend.innerText();
  expect(shown, "the legend must name the unread hue").toMatch(/not read yet/i);
  expect(shown, "the legend must name the partway hue").toMatch(/partway/i);
  expect(shown, "the legend must name the read-through hue").toMatch(/read through/i);
  expect(shown, "the legend must say what the bloom means").toMatch(/glow/i);
  // One line of copy, not a chart.
  expect(shown.length, `the legend ran long: ${shown}`).toBeLessThan(160);

  // It explains the chapter grid too, and must not scroll away from a long one —
  // a legend you have to go looking for is the tooltip problem again.
  await ot(page).click();
  await tile(page, "Ps").click();
  await expect(page.locator(".grid.nums button")).toHaveCount(150);
  await page.locator(".dialog .content").evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(page.locator(".grid.nums button").last()).toBeInViewport();
  await expect(legend, "the legend scrolled away with the grid").toBeVisible();
});
