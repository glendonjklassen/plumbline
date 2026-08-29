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
  await page.getByLabel("Menu").click();
  await expect(
    page.getByRole("button", { name: "Welcome" }).first(),
    "the Welcome entry is gone for good — there is no way back to the intro",
  ).toBeVisible();

  // And the RECORDED intro survives the relaunch, which is where "permanently"
  // was decided.
  await page.reload({ timeout: 45_000 });
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  expect(
    await page.evaluate(() => (window as any).__plumbline?.intro ?? null),
    "the intro was not persisted — recorded this launch and gone the next",
  ).toBe("new");
});

// A reader whose path never recorded an `intro` — the established believer —
// still finds Welcome in the ≡ utilities (it shows for EVERY reader now, and
// falls back to the new-believer welcome; the old conditional entry hid it from
// exactly the reader who never had one, UAT 2026-08-06). It must reopen the
// welcome without touching data.
//
// MUTATION: drop the `?? "new"` fallback on the ≡ Welcome entry in
// Shell.svelte. Red: `reopenIntro` stays null and the poll below times out.
test("the intro is reachable from the ≡ menu when no intro was recorded", async ({ page }) => {
  await firstRun(page);
  await page.getByRole("button", { name: "Established believer" }).click();
  await page.getByRole("button", { name: "Start reading" }).click();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

  // The established path records no `intro` — the entry still shows, and falls
  // back to the new-believer welcome.
  expect(await page.evaluate(() => (window as any).__plumbline?.intro ?? null)).toBeNull();
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Welcome" }).click();

  // The welcome is up. Non-destructive: reopenIntro is shell state, no config
  // path fired — `intro` stays null underneath it.
  await expect
    .poll(() => page.evaluate(() => (window as any).__plumbline?.reopenIntro ?? null))
    .not.toBeNull();
  expect(await page.evaluate(() => (window as any).__plumbline?.intro ?? null)).toBeNull();
});

// THE PROSE PATHS WAIT FOR SOMEONE WHO CAN WRITE THEM.
//
// "New believer" and "Curious about the Bible" are not screens of labels — they
// are somebody speaking to a reader about their own life, and which idioms land
// and which questions are live are things you know by being from a place. So
// they are written by someone inside that culture or they are not written, and
// until they exist in a language the paths that lead to them are not offered.
// The engine decides it (`i18n::Lang::has_native_intros`, derived from whether
// those keys are actually in that language's own catalogue) and the shell obeys.
//
// German is the case in hand: everything else in the app is translated, and
// these two are not, so a German reader used to be invited into them and then
// addressed in English.
//
// CAN FAIL: before the gate, `de.json` carries none of the `intro.welcome.*` or
// `intro.curious.*` keys, so `resolved()` laid English underneath them and both
// cards rendered — with German titles from `intro.pathNew`/`pathCurious`, which
// ARE translated, leading to English paragraphs. The first assertion below sees
// two cards where it wants none.
test.describe("a device with no first-run prose in its language", () => {
  test.use({ locale: "de-DE" });

  test("is offered the paths that exist and not the two that do not", async ({ page }) => {
    await page.goto("/");
    // The established path IS offered — its card is a label and a description,
    // which translate like everything else.
    const established = page.getByRole("button", { name: "Erfahrener Gläubiger" });
    await expect(established).toBeVisible({ timeout: 90_000 });

    // Located by their own German titles, which exist and are translated. The
    // titles being present in `de.json` while the prose behind them is not is
    // exactly the trap: a shell that gated on "is this string translated" would
    // have shown both.
    await expect(page.getByRole("button", { name: "Neu im Glauben" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Neugierig auf die Bibel" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Das Evangelium weitergeben" })).toBeVisible();

    // ...and the menu offers no way back to a welcome this reader was never
    // given. Through the established path, which records no `intro` — the case
    // where the Welcome button used to fall back to the new-believer page.
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).or(page.getByRole("button", { name: "Lesen beginnen" })).click();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
    await page.getByLabel("Menü").or(page.getByLabel("Menu")).click();
    await expect(page.getByRole("button", { name: "Willkommen" })).toHaveCount(0);
  });
});

// The other half, and the one that keeps the gate from being a way to lose the
// feature: English has the prose, so English is offered all four paths.
//
// CAN FAIL: gate the wrong way round (or default `nativeIntros` to false when
// the catalogue is missing a field) and this goes red while the German test
// above stays green — which is why both are here.
test("a language the prose was written in is offered every path", async ({ page }) => {
  await firstRun(page);
  for (const name of ["Curious about the Bible", "New believer", "Sharing the gospel", "Established believer"])
    await expect(page.getByRole("button", { name })).toBeVisible();
});
