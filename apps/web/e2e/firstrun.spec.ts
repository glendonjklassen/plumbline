import { expect, test, type Page } from "@playwright/test";

// A STRAY TAP MUST NOT COST SOMEONE THE WELCOME (audit D-08).
//
// The first-run card is a `<div class="dialog">` with a sibling `.backdrop`, and
// the backdrop's click called `dismiss()`. On the chooser and the tiers page
// that ran `finish()`, which sets `showFirstRun = false` and flushes the config
// — while `config.intro`, the flag the top bar's Welcome button keys off
// (`session.intro`), is written only by `startInJohn()`.
//
// So one miss with a thumb, on the very first screen, closed onboarding
// permanently AND made it unreachable: no Welcome button, and no way back short
// of erasing the reader's data. This is the app's first impression on someone
// who may never have opened a Bible.
//
// HISTORY: this fix was written on 2026-07-29, passed its own tests, and was
// held back UNSHIPPED because with it in the tree `network.spec.ts` went from
// 27 s and 3-of-3 to 4.3 minutes with one test hung to its 240 s timeout — and
// nothing in the code explained how it could. `timedReload` has since been given
// an explicit `page.reload({ timeout: 45_000 })` precisely so that a hang names
// itself instead of eating the budget, which is what made retrying it worthwhile.

/** Straight to the chooser on a clean device. */
async function firstRun(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Established believer" })).toBeVisible({ timeout: 90_000 });
}

/** A click on the backdrop, at a point the card does not cover. `.backdrop` and
 *  `.dialog` are siblings, so this is a real miss rather than a synthesised
 *  event: the top-left corner is outside a centred card at any viewport. */
async function tapOutside(page: Page): Promise<void> {
  await page.locator(".backdrop").click({ position: { x: 4, y: 4 } });
}

// MUTATION: restore the old line in FirstRun.svelte's `dismiss()` —
//   `if (stage !== "welcome" && stage !== "church") finish(human, machine);`
// Red: "a tap outside the chooser closed first run" — the card is gone.
test("a tap outside the chooser answers nothing", async ({ page }) => {
  await firstRun(page);
  await tapOutside(page);

  // Still asking. Nothing has been decided on the reader's behalf.
  await expect(
    page.getByRole("button", { name: "Established believer" }),
    "a tap outside the chooser closed first run — it is a question, and a miss is not an answer",
  ).toBeVisible();
  expect(
    await page.evaluate(() => (window as any).__plumbline?.showFirstRun ?? null),
    "first run was dismissed by a miss",
  ).not.toBe(false);
});

// The tiers page is the same case one step in: it asks a question too, and the
// old code dismissed it.
//
// MUTATION: as above. Red: the tier checkboxes are gone after the tap.
test("a tap outside the tiers page answers nothing", async ({ page }) => {
  await firstRun(page);
  await page.getByRole("button", { name: "Established believer" }).click();
  const start = page.getByRole("button", { name: "Start reading" });
  await expect(start).toBeVisible();

  await tapOutside(page);
  await expect(start, "a tap outside the tiers page closed first run").toBeVisible();
});

// THE ONE THAT MATTERS. Not "was the card dismissed" but "can this reader ever
// reach the welcome again" — the permanence is the bug, not the dismissal.
//
// MUTATION: restore the old `dismiss()` line. Red: "the Welcome button is gone
// for good" — first run closes, `intro` is never written, and `session.intro`
// stays null on this and every future launch.
test("however first run ends, the welcome stays reachable", async ({ page }) => {
  await firstRun(page);
  // The read-and-go path: a tap outside the welcome is "got it", not "undo".
  await page.getByRole("button", { name: "New believer" }).click();
  await expect(page.getByRole("button", { name: "Open the Bible" })).toBeVisible();
  await tapOutside(page);

  // It ended, and it ended by RECORDING itself.
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  expect(
    await page.evaluate(() => (window as any).__plumbline?.intro ?? null),
    "first run ended without recording which welcome was read, so the Welcome button will never appear",
  ).toBe("new");
  await expect(
    page.getByRole("button", { name: "Welcome" }).first(),
    "the Welcome button is gone for good — there is no way back to the intro",
  ).toBeVisible();

  // And it survives the relaunch, which is where "permanently" was decided.
  await page.reload({ timeout: 45_000 });
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(
    page.getByRole("button", { name: "Welcome" }).first(),
    "the intro was not persisted — the button is there this launch and gone the next",
  ).toBeVisible();
});

// A reader whose path never recorded an `intro` — the established believer — has
// no top-bar Welcome button (the previous test is the new-believer twin). Their
// way back is the Settings entry, and it must reopen the welcome without
// touching data (UAT, 2026-08-06). Drives the established path, confirms no
// top-bar button, then reopens from Settings.
//
// MUTATION: make the Settings button's onclick a no-op in SettingsDialog.svelte.
// Red: `reopenIntro` stays null and the poll below times out.
test("the intro is reachable from Settings when no top-bar Welcome exists", async ({ page }) => {
  await firstRun(page);
  await page.getByRole("button", { name: "Established believer" }).click();
  await page.getByRole("button", { name: "Start reading" }).click();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

  // The established path records no `intro`, so the top-bar Welcome button never
  // appears — this reader's only route back is Settings.
  expect(await page.evaluate(() => (window as any).__plumbline?.intro ?? null)).toBeNull();
  await expect(page.getByRole("button", { name: "Welcome" })).toHaveCount(0);

  // Reopen the welcome from Settings.
  await page.evaluate(() => ((window as any).__plumbline.showSettings = true));
  await page.getByRole("button", { name: "Show the welcome" }).click();

  // The welcome is back, and Settings closed. Non-destructive: reopenIntro is
  // shell state, no config path fired.
  await expect
    .poll(() => page.evaluate(() => (window as any).__plumbline?.reopenIntro ?? null))
    .not.toBeNull();
  expect(await page.evaluate(() => (window as any).__plumbline?.showSettings)).toBe(false);
});
