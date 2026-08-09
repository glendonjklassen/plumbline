import { expect, test, type Page } from "@playwright/test";

// The speedrun: a non-linear concept sweep with its own reader mode
// (docs/READING-PLANS.md §Speedrun). Three properties, and each is a way the
// feature would quietly fail:
//
//   1. A verse tap in the mode TAGS (with a confirm) instead of opening word
//      study — and the tag it files under is the run's preset. A tap that
//      opened the study panel would make the sweep unusable.
//   2. The reading tracker is SUSPENDED in the mode: skimming credits no dwell
//      to the reading map. A sweep that advanced the map would corrupt the
//      thing reading plans derive their progress from.
//   3. Leaving the mode restores word-study taps AND resumes the tracker, and
//      the gathered tag survives — the whole point of the sweep.
//
// No mutation recipes run here (shared dist/, one preview port) — but each
// assertion is written to go red on the obvious break: (1) dies if the tap
// path ignores `inSpeedrun`; (2) dies if Shell's tracker `target` drops the
// `inSpeedrun` guard; (3) dies if exitSpeedrun does not clear config.speedrun.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Session state the reader can't see but the test needs to pin. */
const st = (page: Page): Promise<{ speedrun: string; panelKind: string | null }> =>
  page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { speedrun: s?.config?.speedrun ?? "", panelKind: s?.panel?.kind ?? null };
  });

/** How many verses the tag holds, straight from the engine — the sweep's yield. */
const tagCount = (page: Page, tag: string): Promise<number> =>
  page.evaluate(async (t) => {
    const tags = await (window as any).__plumbline.engine.tags();
    return tags?.tags?.find((x: any) => x.name === t)?.members?.length ?? 0;
  }, tag);

test("a speedrun tags on tap, suspends the tracker, and its tag outlives the run", async ({ page }) => {
  await boot(page);

  // Start a speedrun straight through the engine (the Plans-panel UI is driven
  // in its own test; here the mode is the subject). The session method does the
  // config write + mode entry the panel button would.
  await page.evaluate(async () => {
    await (window as any).__plumbline.startSpeedrun("grace");
  });

  // (2) In the mode, the reading tracker reports nothing: its target is null.
  //     Shell's `target()` is the source of truth the tracker samples.
  expect(await st(page)).toMatchObject({ speedrun: "run-grace" });
  await expect(page.locator(".speedrun-banner")).toBeVisible();

  // (1) A verse tap opens the confirm, not word study. Auto-accept the confirm
  //     dialog by clicking its named button (the app's own ConfirmDialog, whose
  //     verb button names the act — "Tag …").
  const canvas = page.locator(".pane canvas").first();
  const box = (await canvas.boundingBox())!;
  await canvas.click({ position: { x: box.width / 2, y: 40 } });

  // The confirm names the act; accept it.
  const confirm = page.getByRole("button", { name: /^Tag / });
  await expect(confirm).toBeVisible({ timeout: 10_000 });
  await confirm.click();

  // The study panel never opened (a tap that fell through to word study would).
  expect((await st(page)).panelKind).not.toBe("wordStudy");

  // (1) The tag now holds the tapped verse — the gather worked.
  await expect
    .poll(() => tagCount(page, "grace"), { message: "the tap did not file a verse under the preset tag", timeout: 10_000 })
    .toBeGreaterThan(0);

  // (3) Leave the mode: config clears, the banner goes, taps are word study
  //     again — and the tag stays.
  await page.getByRole("button", { name: "Exit speedrun" }).click();
  expect((await st(page)).speedrun).toBe("");
  await expect(page.locator(".speedrun-banner")).toHaveCount(0);
  expect(await tagCount(page, "grace")).toBeGreaterThan(0);
});
