import { expect, test, type Page } from "@playwright/test";

// A stray tap must not cost someone the welcome. The first-run card is a
// `<div class="dialog">` with a sibling `.backdrop`, and the backdrop's click called
// `dismiss()`. On the chooser and the tiers page that ran `finish()`, which sets
// `showFirstRun = false` and flushes the config — while `config.intro`, the flag the
// Welcome button keys off (`session.intro`), is written only by `startInJohn()`. So one
// miss with a thumb closed onboarding permanently and made it unreachable: no Welcome
// button, and no way back short of erasing the reader's data.
//
// The reloads below carry an explicit `timeout` so a hang names itself instead of
// eating the whole test budget.

/** Straight to the chooser on a clean device. */
async function firstRun(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Established believer" })).toBeVisible({ timeout: 90_000 });
}

/** A click on the backdrop, at a point the card does not cover. `.backdrop` and
 *  `.dialog` are siblings, so this is a real miss: the top-left corner is outside a
 *  centred card at any viewport. */
async function tapOutside(page: Page): Promise<void> {
  await page.locator(".backdrop").click({ position: { x: 4, y: 4 } });
}

// Fails against the old FirstRun.svelte line
// `if (stage !== "welcome" && stage !== "church") finish(human, machine);` in
// `dismiss()`: the card is gone after a tap outside.
test("a tap outside the chooser answers nothing", async ({ page }) => {
  await firstRun(page);
  await tapOutside(page);

  // Still asking: nothing has been decided on the reader's behalf.
  await expect(
    page.getByRole("button", { name: "Established believer" }),
    "a tap outside the chooser closed first run — it is a question, and a miss is not an answer",
  ).toBeVisible();
  expect(
    await page.evaluate(() => (window as any).__plumbline?.showFirstRun ?? null),
    "first run was dismissed by a miss",
  ).not.toBe(false);
});

// The tiers page is the same case one step in: it asks a question too, and the old code
// dismissed it. Against that `dismiss()` the tier checkboxes are gone after the tap.
test("a tap outside the tiers page answers nothing", async ({ page }) => {
  await firstRun(page);
  await page.getByRole("button", { name: "Established believer" }).click();
  const start = page.getByRole("button", { name: "Start reading" });
  await expect(start).toBeVisible();

  await tapOutside(page);
  await expect(start, "a tap outside the tiers page closed first run").toBeVisible();
});

// The permanence is the bug, not the dismissal: not "was the card dismissed" but "can
// this reader ever reach the welcome again". Against the old `dismiss()` line, first run
// closes, `intro` is never written, and `session.intro` stays null on this and every
// future launch.
test("however first run ends, the welcome stays reachable", async ({ page }) => {
  await firstRun(page);
  // The read-and-go path: a tap outside the welcome is "got it", not "undo".
  await page.getByRole("button", { name: "New believer" }).click();
  await expect(page.getByRole("button", { name: "Open the Bible" })).toBeVisible();
  await tapOutside(page);

  // It ended, and it ended by recording itself.
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

  // And the recorded intro survives the relaunch, which is where the permanence was.
  await page.reload({ timeout: 45_000 });
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  expect(
    await page.evaluate(() => (window as any).__plumbline?.intro ?? null),
    "the intro was not persisted — recorded this launch and gone the next",
  ).toBe("new");
});

// A reader whose path never recorded an `intro` — the established believer — still finds
// Welcome in the ≡ utilities: it shows for every reader and falls back to the
// new-believer welcome, and reopening it must not touch data. Fails against a ≡ Welcome
// entry with no `?? "new"` fallback in Shell.svelte: `reopenIntro` stays null.
test("the intro is reachable from the ≡ menu when no intro was recorded", async ({ page }) => {
  await firstRun(page);
  await page.getByRole("button", { name: "Established believer" }).click();
  await page.getByRole("button", { name: "Start reading" }).click();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

  expect(await page.evaluate(() => (window as any).__plumbline?.intro ?? null)).toBeNull();
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Welcome" }).click();

  // Non-destructive: `reopenIntro` is shell state, no config path fired, so `intro`
  // stays null underneath it.
  await expect
    .poll(() => page.evaluate(() => (window as any).__plumbline?.reopenIntro ?? null))
    .not.toBeNull();
  expect(await page.evaluate(() => (window as any).__plumbline?.intro ?? null)).toBeNull();
});

// The "New believer" and "Curious about the Bible" paths are prose written by someone
// inside the reader's own culture, so a language without that prose is not offered them.
// `i18n::Lang::has_native_intros` decides it from whether those keys are in that
// language's own catalogue, and the shell obeys.
//
// Can fail: without the gate, `de.json` carries none of the `intro.welcome.*` or
// `intro.curious.*` keys, so `resolved()` lays English underneath them and both cards
// render — German titles from `intro.pathNew`/`pathCurious` leading to English
// paragraphs. The first assertion below then sees two cards where it wants none.
test.describe("a device with no first-run prose in its language", () => {
  test.use({ locale: "de-DE" });

  test("is offered the paths that exist and not the two that do not", async ({ page }) => {
    await page.goto("/");
    // The established path is offered: its card is a label and a description, which
    // translate like everything else.
    const established = page.getByRole("button", { name: "Erfahrener Gläubiger" });
    await expect(established).toBeVisible({ timeout: 90_000 });

    // Located by their own German titles, which exist and are translated. That the
    // titles are in `de.json` while the prose behind them is not is the trap: a shell
    // gating on "is this string translated" would have shown both.
    await expect(page.getByRole("button", { name: "Neu im Glauben" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Neugierig auf die Bibel" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Das Evangelium weitergeben" })).toBeVisible();

    // And no way back to a welcome this reader was never given, through the established
    // path that records no `intro` — where the Welcome button falls back otherwise.
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).or(page.getByRole("button", { name: "Lesen beginnen" })).click();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
    await page.getByLabel("Menü").or(page.getByLabel("Menu")).click();
    await expect(page.getByRole("button", { name: "Willkommen" })).toHaveCount(0);
  });
});

// The other half, keeping the gate from becoming a way to lose the feature: English has
// the prose, so English is offered all four paths. Gate it the wrong way round (or
// default `nativeIntros` to false when the catalogue lacks the field) and this goes red
// while the German test above stays green, which is why both are here.
test("a language the prose was written in is offered every path", async ({ page }) => {
  await firstRun(page);
  for (const name of ["Curious about the Bible", "New believer", "Sharing the gospel", "Established believer"])
    await expect(page.getByRole("button", { name })).toBeVisible();
});
