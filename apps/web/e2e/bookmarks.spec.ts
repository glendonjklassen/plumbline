import { expect, test, type Page } from "@playwright/test";

// The bookmarks row (maintainer ask, 2026-08-24; icon-only 2026-08-25, passage
// restored the same day): the row above the canon strip — grown out of the plan
// chip — carries a pill chip per stored bookmark: the running plan, then every
// seating position in `config.slots` that the row asks for — Sunday morning
// alone now (Last opened stood down 2026-08-26 with the two evenings: the app
// already reopens where the reader left off, so it was a chip naming the place
// they were standing). Each face is an ICON naming the kind beside the
// PASSAGE it holds ("Psalms 23:4"); the kind's NAME rides aria-label/title, and
// a tap toasts WHICH bookmark it was before navigating. Several chips show at
// once, so it is plain there are more than one and that the row scrolls when
// they overflow.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1100, height: 800 });
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

test("a stored seating is a chip naming its passage; a tap says which bookmark and goes there", async ({ page }) => {
  await boot(page);
  // Plant a Sunday-morning seating other than where the reader is.
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.slots = { ...(s.config.slots ?? {}), "sunday-morning": { book: "Ps", chapter: 23, verse: 4 } };
  });

  const tile = page.locator('.bm-tile[data-slot="sunday-morning"]');
  // The face is the icon AND the passage — where the tap goes is the reason
  // to tap (a morning of icon-only chips said which, not where). The kind's
  // name stays off the face and on the accessible name.
  await expect(tile).toHaveAttribute("aria-label", "Sunday morning · Psalms 23:4");
  await expect(tile.locator("svg")).toHaveCount(1);
  await expect(tile).toHaveText("Psalms 23:4");
  expect(await tile.textContent(), "the kind's name belongs to the label and the toast, not the chip").not.toContain(
    "Sunday",
  );

  await tile.click();
  // The toast names the BOOKMARK, plainly — no "going to…" sentence; the
  // destination shows itself when the pane lands there.
  await expect(page.locator(".toast")).toHaveText("Sunday morning bookmark");
  await expect(page.locator(".subtitle")).toHaveText("Psalms 23", { timeout: 30_000 });
});

test("the last-opened seating is stored but is not a chip", async ({ page }) => {
  // The stand-down, from the outside: the engine still records where the reader
  // last was — that is what reopens the app in the right place — and the row
  // draws nothing for it. FAILS against the bug it describes because putting
  // `other` back in PlanChip's SLOT_ORDER is precisely what makes this tile
  // exist; the seating is planted here, so an empty row is the row's choice and
  // not a missing input.
  await boot(page);
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.slots = { other: { book: "Rom", chapter: 8 }, "sunday-morning": { book: "Ps", chapter: 23 } };
  });
  // Wait for the row to settle on the seating that IS asked for, so the absence
  // below is a rendered absence rather than a frame that had not arrived yet.
  await expect(page.locator('.bm-tile[data-slot="sunday-morning"]')).toBeVisible();
  await expect(page.locator('.bm-tile[data-slot="other"]')).toHaveCount(0);
});

test("several chips are visible at once, not one page at a time", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 740 }); // a phone, where one-at-a-time hid the rest
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.config.slots = {
      other: { book: "Rom", chapter: 8 },
      "sunday-morning": { book: "Ps", chapter: 23 },
      "sunday-evening": { book: "John", chapter: 17 },
      "wednesday-evening": { book: "Acts", chapter: 2 },
    };
    // A running plan, so the row holds MORE THAN ONE chip to lay out. Three of
    // the four seatings are stood down, so bookmarks alone can no longer make a
    // crowded row — and a mixed row is the truer subject anyway: the plan chip
    // and the bookmark tiles share the strip and its scroll.
    await s.author("planStart", "nt-90", new Date().toISOString());
  });
  // Only the seating the row asks for became a tile: the three stood-down ones
  // are stored by the engine and drawn by nobody (PlanChip's SLOT_ORDER).
  await expect(page.locator(".bm-tile")).toHaveCount(1);
  const chips = page.locator(".plan-chip-row .plan-chip, .plan-chip-row .bm-tile");
  await expect(chips).toHaveCount(2, { timeout: 10_000 });
  // With a passage on every face the chips need not all fit a 360px row — that
  // is what the scroll is for — but MORE THAN ONE must show whole without
  // scrolling (the pager's failing), and the last must be reachable by it.
  await expect(chips.nth(0)).toBeInViewport({ ratio: 1 });
  await expect(chips.nth(1)).toBeInViewport({ ratio: 1 });
  await chips.last().scrollIntoViewIfNeeded();
  await expect(chips.last()).toBeInViewport({ ratio: 1 });
});
