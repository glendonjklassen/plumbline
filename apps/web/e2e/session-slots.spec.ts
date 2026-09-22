import { expect, test, type Page } from "@playwright/test";

// One "last chapter" is not enough: weekday study, a Sunday service and a midweek meeting are
// three separate places a reader was, so a position is kept per seating and each is picked up
// where it was left.
//
// The rule for which seating a moment falls in lives in the core (`core::session_slot`), asked
// through the engine with the reader's own local date and hour — a slot computed in UTC would put
// a Sunday-evening service in Monday for half the world.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1100, height: 800 });
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

const slotOf = (page: Page, date: string, hour: number): Promise<string> =>
  page.evaluate(([d, h]) => (window as any).__plumbline.rpc.static("sessionSlot", d, h), [date, hour] as const);

test("the core decides which seating a moment is, and both shells ask it", async ({ page }) => {
  await boot(page);
  // 2026-08-16 is a Sunday; 08-19 a Wednesday; 08-18 a Tuesday.
  expect(await slotOf(page, "2026-08-16", 9)).toBe("sunday-morning");
  expect(await slotOf(page, "2026-08-16", 11)).toBe("sunday-morning");
  // Noon is the evening side of the split.
  expect(await slotOf(page, "2026-08-16", 12)).toBe("sunday-evening");
  expect(await slotOf(page, "2026-08-19", 19)).toBe("wednesday-evening");
  // Wednesday morning is deliberately not a slot; the slot exists for the midweek meeting.
  expect(await slotOf(page, "2026-08-19", 9)).toBe("other");
  expect(await slotOf(page, "2026-08-18", 9)).toBe("other");
});

test("a Sunday service time redraws the Sunday seating as its window", async ({ page }) => {
  await boot(page);
  const at = (d: string, min: number, svc: number): Promise<string> =>
    page.evaluate(
      ([dd, m, sv]) => (window as any).__plumbline.rpc.static("sessionSlotAt", dd, m, sv),
      [d, min, svc] as const,
    );
  // Church at 10:30 (630 minutes): the seating runs from half an hour BEFORE the start — arriving
  // is being there (maintainer, 2026-09-21) — until 1.5 hours after it, so an early Sunday riser
  // resumes their ordinary reading rather than last week's service.
  expect(await at("2026-08-16", 9 * 60 + 59, 630)).toBe("other");
  expect(await at("2026-08-16", 10 * 60, 630)).toBe("sunday-morning");
  expect(await at("2026-08-16", 10 * 60 + 30, 630)).toBe("sunday-morning");
  expect(await at("2026-08-16", 11 * 60 + 59, 630)).toBe("sunday-morning");
  expect(await at("2026-08-16", 12 * 60, 630)).toBe("sunday-evening");
  // An afternoon congregation's window outranks the noon split.
  expect(await at("2026-08-16", 13 * 60 + 30, 13 * 60)).toBe("sunday-morning");
  // -1 is "never set": the before-noon rule stands, exactly as above.
  expect(await at("2026-08-16", 9 * 60, -1)).toBe("sunday-morning");
});

test("a passage read now is remembered against this seating", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Ps", 23));
  await expect(page.locator(".subtitle")).toHaveText("Psalms 23", { timeout: 30_000 });

  const saved = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    // The slot is written by the config snapshot, so flush past the debounce.
    s.flushConfig();
    return { slot: s.slot, slots: s.config.slots };
  });
  // Whichever seating the test machine's clock is in: the test must not assume the day it runs on.
  expect(saved.slot).toBeTruthy();
  expect(saved.slots[saved.slot]).toMatchObject({ book: "Ps", chapter: 23 });
});

test("a stored seating is what reopens, over the plain last position", async ({ page }) => {
  await boot(page);
  // Seed: this seating remembers Romans 8, while the last position is elsewhere. Written straight
  // to the home (configSave), not through saveConfig — every shell save refreshes this seating's
  // slot from the live pane, so a divergent slot has to be planted below the snapshot, with
  // `restoring` keeping the shell's own pagehide flush off it.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const cfg = JSON.parse(JSON.stringify(s.config));
    cfg.slots = { ...(cfg.slots ?? {}), [s.slot]: { book: "Rom", chapter: 8 } };
    cfg.openPanes = [{ book: "John", chapter: 3 }];
    await s.rpc.static("configSave", cfg);
    await s.rpc.flush();
    s.restoring = true;
  });

  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator(".subtitle")).toHaveText("Romans 8", { timeout: 30_000 });
});

test("a seating never used falls through to the plain last position", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    // A slot table with an entry for a different seating than this one.
    const other = s.slot === "other" ? "sunday-morning" : "other";
    const cfg = JSON.parse(JSON.stringify(s.config));
    cfg.slots = { [other]: { book: "Rev", chapter: 22 } };
    cfg.openPanes = [{ book: "John", chapter: 3 }];
    await s.rpc.static("configSave", cfg);
    await s.rpc.flush();
    s.restoring = true;
  });

  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator(".subtitle")).toHaveText("John 3", { timeout: 30_000 });
});

// Fails against a slot marked chapter-only at navigation time, with only openPanes carrying the
// first-visible verse: the slot wins the restore, so every same-seating reopen found its chapter
// and landed at the top. Red at the first assertion — the flushed slot carries no verse.
test("a reopened seating restores the scroll position, not just the chapter", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Ps", 119));
  await expect(page.locator(".subtitle")).toHaveText("Psalms 119", { timeout: 30_000 });

  // scrollTop clamps to 0 until the layout has grown the container. Ps 119 has 176 verses and the
  // John 3 the app booted into has 36, so >100 is the new layout rather than the old one.
  await expect
    .poll(
      () => page.evaluate(() => (window as any).__plumbline.paneVerseGeom[0]?.size ?? 0),
      { timeout: 60_000 },
    )
    .toBeGreaterThan(100);
  // Scroll the way a reader does: the browser fires the scroll event and ReaderPane's user branch
  // takes the offset from there.
  await page.evaluate(() => {
    (document.querySelector(".pane .scroll") as HTMLElement).scrollTop = 4000;
  });
  await expect
    .poll(() => page.evaluate(() => (window as any).__plumbline.panes[0].scrollY))
    .toBeGreaterThan(0);

  // The flush must be awaited before the reload: flushConfig only queues the configSave, and the
  // worker's persist of it is fire-and-forget, so a reload that outruns the IndexedDB write
  // reopens on the previous, verse-less save.
  const savedVerse = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.flushConfig();
    await s.rpc.flush();
    return s.config.slots?.[s.slot]?.verse;
  });
  expect(savedVerse).toBeGreaterThan(1);

  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator(".subtitle")).toHaveText("Psalms 119", { timeout: 30_000 });

  // The verse it closed on is back at the top edge, computed the shell's own way: an unconsumed
  // scroll target is the position (pendingScroll holds until the reader scrolls), and after that
  // it is read off the pane's published verse geometry.
  await expect
    .poll(
      () =>
        page.evaluate(() => {
          const s = (window as any).__plumbline;
          const pane = s.panes[0];
          const geom = s.paneVerseGeom[0];
          if (!pane || !geom || pane.scrollY <= 0) return null;
          if (pane.pendingScroll && pane.targetVerse > 1) return pane.targetVerse;
          let best: number | null = null;
          let bestY = Infinity;
          for (const [v, g] of geom)
            if (g.y + g.h > pane.scrollY && g.y < bestY) {
              bestY = g.y;
              best = v;
            }
          return best;
        }),
      { timeout: 30_000 },
    )
    .toBe(savedVerse);
});

// ── the seating is asked again on a return to the foreground ──────────────────────────────────
//
// An installed PWA is almost never launched; it is brought back. Resolved once per launch, the
// seating on Sunday in the pew was still Saturday night's: nothing restored, and the whole service
// was saved to the everyday slot (maintainer, 2026-09-21: "when I'm at church on Sunday morning,
// it's not working"). The clock is the page's own `Date`, which Playwright can fix; the seating is
// asked of the engine WITH that date and minute, so the engine itself needs no faking.

const SATURDAY_NIGHT = new Date("2026-09-19T21:00:00"); // local time: a Saturday
const SUNDAY_IN_THE_PEW = new Date("2026-09-20T10:20:00"); // ten minutes before a 10:30 service
const CHURCH_AT = 10 * 60 + 30;

/** Come back to the foreground: the event the app listens for, fired by hand. A headless page is
 *  already "visible", so the listener's own check passes. */
const resume = (page: Page) =>
  page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));

// Can fail: without the foreground listener the slot never changes after boot, so the subtitle
// stays on Genesis 5 and the Psalms expectation times out.
test("a return to the foreground in a new seating reopens that seating's place", async ({ page }) => {
  await page.clock.setFixedTime(SATURDAY_NIGHT);
  await boot(page);
  expect(await page.evaluate(() => (window as any).__plumbline.slot)).toBe("other");
  await page.evaluate((svc) => {
    const s = (window as any).__plumbline;
    s.config.sundayService = svc;
    // Last Sunday's place, planted: the reader has not been there this session.
    s.config.slots = { ...(s.config.slots ?? {}), "sunday-morning": { book: "Ps", chapter: 23 } };
    s.navigate(0, "Gen", 5);
  }, CHURCH_AT);
  await expect(page.locator(".subtitle")).toHaveText("Genesis 5", { timeout: 30_000 });
  // Putting the phone down: the hide flush files Saturday's place under Saturday's seating.
  await page.evaluate(() => (window as any).__plumbline.flushConfig());

  await page.clock.setFixedTime(SUNDAY_IN_THE_PEW);
  await resume(page);
  await expect(page.locator(".subtitle")).toHaveText("Psalms 23", { timeout: 30_000 });
  expect(await page.evaluate(() => (window as any).__plumbline.slot)).toBe("sunday-morning");
  // Saturday's place is still Saturday's: the restore reads the new seating and does not rewrite
  // the old one.
  await page.evaluate(() => (window as any).__plumbline.flushConfig());
  expect(await page.evaluate(() => (window as any).__plumbline.config.slots)).toMatchObject({
    other: { book: "Gen", chapter: 5 },
    "sunday-morning": { book: "Ps", chapter: 23 },
  });
});

// Can fail: without the write-side re-ask in saveConfig, the slot is still "other" when the
// foreground asks, the answer differs, and the planted Psalm 23 is restored over Romans 8.
test("a window that opens while the reader is here moves the seating, not the reader", async ({ page }) => {
  await page.clock.setFixedTime(SATURDAY_NIGHT);
  await boot(page);
  await page.evaluate((svc) => {
    const s = (window as any).__plumbline;
    s.config.sundayService = svc;
    s.config.slots = { ...(s.config.slots ?? {}), "sunday-morning": { book: "Ps", chapter: 23 } };
  }, CHURCH_AT);

  // The window opens under a page that stays in the foreground: the reader turns to the sermon's
  // passage at 10:20 with the app already open. The save that navigation makes asks the seating
  // first, so the slot moves with it.
  await page.clock.setFixedTime(SUNDAY_IN_THE_PEW);
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Rom", 8));
  await expect(page.locator(".subtitle")).toHaveText("Romans 8", { timeout: 30_000 });
  await expect.poll(() => page.evaluate(() => (window as any).__plumbline.slot)).toBe("sunday-morning");

  // A lock and unlock mid-sermon: the seating is unchanged, so nothing is restored. The answer
  // is a few ms away; the wait is for the negative, which has no event to hang on.
  await resume(page);
  await page.waitForTimeout(500);
  await expect(page.locator(".subtitle")).toHaveText("Romans 8");
  await page.evaluate(() => (window as any).__plumbline.flushConfig());
  expect(
    await page.evaluate(() => (window as any).__plumbline.config.slots["sunday-morning"]),
  ).toMatchObject({ book: "Rom", chapter: 8 });
});
