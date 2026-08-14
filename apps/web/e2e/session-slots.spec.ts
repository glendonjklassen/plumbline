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

test("a passage read now is remembered against this seating", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Ps", 23));
  await expect(page.locator(".subtitle")).toHaveText("Psalms 23", { timeout: 30_000 });

  const saved = await page.evaluate(() => {
    const s = (window as any).__plumbline;
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
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.slots = { ...(s.config.slots ?? {}), [s.slot]: { book: "Rom", chapter: 8 } };
    s.config.openPanes = [{ book: "John", chapter: 3 }];
    s.saveConfig();
  });
  // Let the debounced write reach the worker before the reload races it.
  await page.waitForTimeout(400);
  await page.evaluate(async () => {
    await (window as any).__plumbline.rpc.static("routeLink", "go:John:3:16");
  });

  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  // Romans 8 — the seating's own passage — not John 3.
  await expect(page.locator(".subtitle")).toHaveText("Romans 8", { timeout: 30_000 });
});

test("a seating never used falls through to the plain last position", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    // A slot table with an entry for a DIFFERENT seating than this one.
    const other = s.slot === "other" ? "sunday-morning" : "other";
    s.config.slots = { [other]: { book: "Rev", chapter: 22 } };
    s.config.openPanes = [{ book: "John", chapter: 3 }];
    s.saveConfig();
  });
  await page.waitForTimeout(400);
  await page.evaluate(async () => {
    await (window as any).__plumbline.rpc.static("routeLink", "go:John:3:16");
  });

  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  // John 3 stands: this is exactly what every reader gets today, and a slot
  // they have never sat in must not change it.
  await expect(page.locator(".subtitle")).toHaveText("John 3", { timeout: 30_000 });
});
