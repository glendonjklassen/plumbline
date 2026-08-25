import { expect, test, type Page } from "@playwright/test";

// A reader's last chapter is not one thing. Somebody who studies on weekday
// mornings, sits in a Sunday service and goes to a Wednesday meeting has three
// separate places they were, and one "last chapter" serves whichever they did
// most recently — so arriving at church reopened Saturday night's study
// (maintainer, 2026-08-13). A position per SEATING picks each thread up where
// it was left.
//
// The RULE for which seating a moment falls in lives in the core
// (`core::session_slot`), asked through the engine with the reader's own LOCAL
// date and hour — a slot computed in UTC would put a Sunday-evening service in
// Monday for half the world.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1100, height: 800 });
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
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
  // Wednesday MORNING is deliberately not a slot: it is a weekday morning like
  // any other, and the slot exists for the midweek meeting.
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
  // Church at 10:30 (630 minutes): the seating runs from the start until 1.5
  // hours after — before it, an early Sunday riser resumes their ordinary
  // reading, not last week's service.
  expect(await at("2026-08-16", 10 * 60 + 29, 630)).toBe("other");
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
  // Whichever seating the test machine's clock is in, the passage is filed
  // under it — the test must not assume the day it runs on.
  expect(saved.slot).toBeTruthy();
  expect(saved.slots[saved.slot]).toMatchObject({ book: "Ps", chapter: 23 });
});

test("a stored seating is what reopens, over the plain last position", async ({ page }) => {
  await boot(page);
  // Seed: this seating remembers Romans 8, while the last position is elsewhere.
  // Written straight to the home (configSave), not through saveConfig: every
  // shell save refreshes this seating's slot from the live pane — the very
  // behaviour under test — so a divergent slot has to be planted below the
  // snapshot, with `restoring` keeping the shell's own pagehide flush off it.
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
  // Romans 8 — the seating's own passage — not John 3.
  await expect(page.locator(".subtitle")).toHaveText("Romans 8", { timeout: 30_000 });
});

test("a seating never used falls through to the plain last position", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    // A slot table with an entry for a DIFFERENT seating than this one.
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
  // John 3 stands: this is exactly what every reader gets today, and a slot
  // they have never sat in must not change it.
  await expect(page.locator(".subtitle")).toHaveText("John 3", { timeout: 30_000 });
});

// The bug this guards against (2026-08-21): the slot was marked chapter-only at
// navigation time, while only openPanes carried the first-visible verse — and
// the slot WINS the restore. So every same-seating reopen (close the app, open
// it again an hour later) found its chapter but landed at the top. Red without
// the fix at the first assertion: the flushed slot carries no verse.
test("a reopened seating restores the scroll position, not just the chapter", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Ps", 119));
  await expect(page.locator(".subtitle")).toHaveText("Psalms 119", { timeout: 30_000 });

  // Wait for the fresh chapter's verse geometry — scrollTop clamps to 0 until
  // the layout has grown the container. Ps 119 has 176 verses; the John 3 the
  // app booted into has 36, so >100 is the NEW layout, not the old one.
  await expect
    .poll(
      () => page.evaluate(() => (window as any).__plumbline.paneVerseGeom[0]?.size ?? 0),
      { timeout: 60_000 },
    )
    .toBeGreaterThan(100);
  // Scroll well into the chapter the way a reader does — the browser fires the
  // scroll event; ReaderPane's user branch takes the offset from there.
  await page.evaluate(() => {
    (document.querySelector(".pane .scroll") as HTMLElement).scrollTop = 4000;
  });
  await expect
    .poll(() => page.evaluate(() => (window as any).__plumbline.panes[0].scrollY))
    .toBeGreaterThan(0);

  // What the close is about to bank: this seating's slot, VERSE INCLUDED.
  // The flush is AWAITED before the reload: flushConfig only queues the
  // configSave, and the worker's own persist of it is fire-and-forget — a
  // reload that outruns the IndexedDB write reopens on the previous,
  // verse-less save and this test goes red with the fix in place (it did,
  // twice, on the v0.60.1 tag run).
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

  // Mid-chapter, with the verse it closed on back at the top edge — not the top
  // of the chapter. Computed the shell's own way: an unconsumed scroll target IS
  // the position (pendingScroll holds until the reader scrolls, by design);
  // after that it is read off the pane's published verse geometry.
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
