import { expect, test, type Page } from "@playwright/test";

// The bookmarks row (maintainer ask, 2026-08-24; icon-only 2026-08-25, passage
// restored the same day): the row above the canon strip — grown out of the plan
// chip — carries a pill chip per stored bookmark: the running plans and
// devotionals. The seating positions in `config.slots` were chips here too and
// are all stood down now — Last opened and the two evenings on 2026-08-26 (the
// app already reopens where the reader left off, so it was a chip naming the
// place they were standing), Sunday morning on 2026-09-21 ("there should still
// be a Sunday morning bookmark so I open it at the right place on Sunday, I just
// don't need to see it as a card on the reader screen"). The bookmark is the
// seating itself (session-slots.spec.ts); the History sheet marks the runs read
// in it (history.spec.ts). Each remaining face is an ICON naming the kind
// beside WHAT IT HOLDS; the kind's NAME rides aria-label/title. Several chips
// show at once, so it is plain there are more than one and that the row scrolls
// when they overflow.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1100, height: 800 });
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

test("a stored seating is not a chip: the bookmark is the seating itself", async ({ page }) => {
  // The stand-down, from the outside: the engine still records every seating —
  // that is what reopens the app in the right place — and the row draws nothing
  // for any of them. FAILS against the bug it describes because putting a
  // seating back in PlanChip's SLOT_ORDER is precisely what makes its tile
  // exist; all four are planted here, so an empty row is the row's choice and
  // not a missing input.
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.config.slots = {
      other: { book: "Rom", chapter: 8 },
      "sunday-morning": { book: "Ps", chapter: 23, verse: 4 },
      "sunday-evening": { book: "John", chapter: 17 },
      "wednesday-evening": { book: "Acts", chapter: 2 },
    };
    // A running plan, so the row RENDERS — the absence below is then a rendered
    // absence in a row that had every seating to draw, not a row that never
    // existed.
    await s.author("planStart", "nt-90", new Date().toISOString());
  });
  await expect(page.locator(".plan-chip-row .plan-chip")).toHaveCount(1, { timeout: 10_000 });
  await expect(page.locator(".bm-tile")).toHaveCount(0);
});

test("several chips are visible at once, not one page at a time", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 740 }); // a phone, where one-at-a-time hid the rest
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    // Two running plans, so the row holds MORE THAN ONE chip to lay out. The
    // seatings are all stood down, so bookmarks can no longer make a crowded
    // row — and plan chips share the strip and its scroll all the same.
    await s.author("planStart", "nt-90", new Date().toISOString());
    await s.author("planStart", "bible-365", new Date().toISOString());
  });
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
