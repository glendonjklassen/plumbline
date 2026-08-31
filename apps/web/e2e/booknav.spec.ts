import { expect, test, type Page } from "@playwright/test";

// The passage navigator: a testament toggle so one testament is listed at a time, and
// a "you are here" marker on the book the pane is showing.
//
// Selectors are data attributes on purpose: `.grid button` is shared by the book grid,
// the chapter grid and half the dialogs, so a class here would let a test come back
// green about something it never looked at. The grid is asserted as ONE snapshot
// rather than a series of `toHaveCount(0)`s — a bare "no Genesis tile" is equally
// satisfied by the dialog having closed, which has happened.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  // A fresh profile opens at John 3, which makes the testament default and the
  // "you are here" marker testable without seeding anything.
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

/** What the book grid shows now: the pressed testament, the book count, two landmark
 *  books, and which tiles claim to be where the reader is. */
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

// Dies if BookNav lists both testaments at once instead of the reader's one: the
// snapshot comes back with count 66 and Genesis present.
test("the testament toggle switches which books are listed, by mouse and by key", async ({ page }) => {
  await boot(page);
  await openNav(page);

  // John 3 is where the reader stands, so the navigator opens in the NT.
  await expect
    .poll(() => grid(page), {
      message: "the navigator should open on the testament the reader is in",
      // The grid paints from the boot-prefetched TOC, still a query in flight for a
      // tick or two on a cold CI boot.
      timeout: 15_000,
    })
    .toEqual({ ...NT_ONLY, marked: ["John"] });
  await expect(tile(page, "Rev")).toBeVisible();

  await ot(page).click();
  await expect
    .poll(() => grid(page), { message: "Old Testament should list the OT, and only the OT" })
    .toEqual({ ...OT_ONLY, marked: [] });
  await expect(tile(page, "Mal")).toBeVisible();

  // The toggle must work from the keyboard, not only from a thumb.
  await nt(page).focus();
  await page.keyboard.press("Enter");
  await expect
    .poll(() => grid(page), { message: "the toggle should work from the keyboard" })
    .toEqual({ ...NT_ONLY, marked: ["John"] });

  // Once the navigator is open the tab belongs to the reader, not the pane: it
  // survives stepping into a book and back out.
  await ot(page).click();
  await tile(page, "Gen").click();
  await expect(page.locator(".grid.nums button")).toHaveCount(50);
  await page.locator(".dialog button.crumb").click();
  await expect
    .poll(() => grid(page), { message: "stepping back should return to the tab the reader chose" })
    .toEqual({ ...OT_ONLY, marked: [] });
});

// Dies if the `aria-current` binding comes off the book tile: nothing is marked.
test("the book the reader is in is marked, and only that one", async ({ page }) => {
  await boot(page);
  await openNav(page);

  // Semantics, not a colour: the reading tint paints every tile, so a test looking at
  // a background would be satisfied by the map alone.
  await expect
    .poll(() => grid(page), {
      message: "exactly one tile should say where the reader is",
      timeout: 15_000,
    })
    .toEqual({ ...NT_ONLY, marked: ["John"] });

  // Nothing is marked in the other testament, and the count proves the grid is still
  // there while nothing is marked.
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

// The colour legend is absent on purpose: a row of colour words above the grid is
// chrome in front of picking a book. The assertion is inverted rather than deleted,
// to keep the legend from drifting back in. Also holds that a long chapter grid still
// scrolls to its end.
test("the navigator is the grid, with no colour legend above it", async ({ page }) => {
  await boot(page);
  await openNav(page);

  await expect(page.locator("[data-tint-legend]")).toHaveCount(0);
  // Nor the copy by any other route: no tint words in the dialog's rendered text.
  const shown = await page.locator(".dialog").innerText();
  expect(shown, "the tint copy came back into the navigator").not.toMatch(/not read yet|partway|read through/i);

  // The tiles keep their own explanation in a `title`.
  await ot(page).click();
  await expect(tile(page, "Ps")).toHaveAttribute("title", /.+/);

  await tile(page, "Ps").click();
  await expect(page.locator(".grid.nums button")).toHaveCount(150);
  await page.locator(".dialog .content").evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(page.locator(".grid.nums button").last()).toBeInViewport();
});

// Mark-as-read lives on the navigator: a long-press or right-click on a chapter tile
// marks it read. Right-click is the deterministic stand-in for the long-press, and
// the read must reach the engine, not just the tile.
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
