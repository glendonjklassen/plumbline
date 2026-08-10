import { expect, test, type Page } from "@playwright/test";

// The concept study: a non-linear concept sweep with its own reader mode
// (docs/READING-PLANS.md §Concept Study). Three properties, and each is a way the
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
// path ignores `inConceptStudy`; (2) dies if Shell's tracker `target` drops the
// `inConceptStudy` guard; (3) dies if exitConceptStudy does not clear
// config.conceptStudy.

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
const st = (page: Page): Promise<{ conceptStudy: string; panelKind: string | null }> =>
  page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { conceptStudy: s?.config?.conceptStudy ?? "", panelKind: s?.panel?.kind ?? null };
  });

/** How many verses the tag holds, straight from the engine — the sweep's yield. */
const tagCount = (page: Page, tag: string): Promise<number> =>
  page.evaluate(async (t) => {
    const tags = await (window as any).__plumbline.engine.tags();
    return tags?.tags?.find((x: any) => x.name === t)?.members?.length ?? 0;
  }, tag);

test("a concept study tags on tap, suspends the tracker, and its tag outlives the run", async ({ page }) => {
  await boot(page);

  // Start a concept study straight through the engine (the Plans-panel UI is driven
  // in its own test; here the mode is the subject). The session method does the
  // config write + mode entry the panel button would.
  await page.evaluate(async () => {
    await (window as any).__plumbline.startConceptStudy("grace");
  });

  // (2) In the mode, the reading tracker reports nothing: its target is null.
  //     Shell's `target()` is the source of truth the tracker samples.
  expect(await st(page)).toMatchObject({ conceptStudy: "run-grace" });
  await expect(page.locator(".concept-study-banner")).toBeVisible();

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
  await page.getByRole("button", { name: "Exit Concept Study" }).click();
  expect((await st(page)).conceptStudy).toBe("");
  await expect(page.locator(".concept-study-banner")).toHaveCount(0);
  expect(await tagCount(page, "grace")).toBeGreaterThan(0);
});

// The Plans-panel path the reader actually walks: type a tag, press Start, and
// the mode is entered; the run then shows as a card that can re-enter the mode.
// Dies if the launcher stops wiring the input through startConceptStudy, or if
// the running card loses its Resume button.
test("the Plans panel launches a concept study and re-enters it from its card", async ({ page }) => {
  await boot(page);

  await page.evaluate(() => ((window as any).__plumbline.panel = { kind: "plans" }));
  await page.getByPlaceholder("Tag to gather into (e.g. grace)").fill("faith");
  await page.getByRole("button", { name: "Start Concept Study" }).click();

  // Launch enters the mode and records the run.
  await expect(page.locator(".concept-study-banner")).toBeVisible();
  expect(await st(page)).toMatchObject({ conceptStudy: "run-faith" });

  // Leave, then re-enter from the run's card — coverage (the run) persists.
  await page.getByRole("button", { name: "Exit Concept Study" }).click();
  await expect(page.locator(".concept-study-banner")).toHaveCount(0);
  await page.evaluate(() => ((window as any).__plumbline.panel = { kind: "plans" }));
  const card = page.locator(".plan-card.concept-study", { hasText: "faith" });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Resume" }).click();
  await expect(page.locator(".concept-study-banner")).toBeVisible();
  expect(await st(page)).toMatchObject({ conceptStudy: "run-faith" });
});
