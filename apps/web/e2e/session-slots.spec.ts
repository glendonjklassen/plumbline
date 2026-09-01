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
  // Church at 10:30 (630 minutes): the seating runs from the start until 1.5 hours after, so an
  // early Sunday riser resumes their ordinary reading rather than last week's service.
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
