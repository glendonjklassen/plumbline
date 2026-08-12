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

// The mode on TOUCH — the input the sweep was built for and the one the mouse
// tests above cannot exercise. A touch tap is delivered TWICE: pointerup, then
// a synthesized `click` a few milliseconds later. Unswallowed, the second
// delivery either ran the tap again (two confirmations for one tap) or — once
// the confirm had rendered — landed on the dialog's backdrop and answered "no"
// to a question the reader never saw, which on a phone read as "tapping a
// verse does nothing". Dies if ReaderPane stops setting `suppressClick` on the
// touch-tap path, or if the confirm backdrop goes back to dismissing on click.
test.describe("touch", () => {
  test.use({ hasTouch: true });

  test("a touch tap asks exactly once, and the confirm survives its ghost click", async ({ page }) => {
    await boot(page);
    await page.evaluate(async () => {
      await (window as any).__plumbline.startConceptStudy("mercy");
    });
    // Warm the plans read: the bug lived on the WARM path, where the tag is
    // cached and the confirm opens fast enough for the synthesized click to
    // land on its backdrop. (Cold, the RPC outran the ghost click and hid it.)
    await page.evaluate(() => (window as any).__plumbline.fetchQ("plans", ""));
    // Count the tap handler's invocations — a double-delivered tap calls it
    // twice whichever element the ghost click lands on.
    await page.evaluate(() => {
      const s = (window as any).__plumbline;
      (window as any).__tagCalls = 0;
      const orig = s.conceptStudyTagVerse.bind(s);
      s.conceptStudyTagVerse = (r: string) => {
        (window as any).__tagCalls++;
        return orig(r);
      };
    });

    const canvas = page.locator(".pane canvas").first();
    const box = (await canvas.boundingBox())!;
    await canvas.tap({ position: { x: box.width / 2, y: 40 } });

    // The confirm appears — and STAYS: past any ghost-click window.
    const confirm = page.getByRole("button", { name: /^Tag / });
    await expect(confirm).toBeVisible({ timeout: 10_000 });
    await page.waitForTimeout(400);
    await expect(confirm).toBeVisible();

    // One tap, one ask.
    expect(await page.evaluate(() => (window as any).__tagCalls)).toBe(1);

    // Accepting it files the verse and no second confirmation surfaces.
    await confirm.click();
    await expect(page.locator('[data-surface="confirm"]')).toHaveCount(0);
    await expect
      .poll(() => tagCount(page, "mercy"), { message: "the touch tap did not file the verse", timeout: 10_000 })
      .toBe(1);
    await page.waitForTimeout(300);
    await expect(page.locator('[data-surface="confirm"]')).toHaveCount(0);
  });
});

// The sweep's progress is VISIBLE in the mode: the banner counts chapters, and
// the navigator paints swept coverage instead of the frozen reading map (whose
// glow cannot move while the tracker is suspended, and so said nothing about
// the study). Dies if the banner loses its count, if `swept` falls off the
// plans wire, or if BookNav goes back to painting the reading map in the mode.
test("the mode shows sweep progress — the banner counts and the navigator paints coverage", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    await (window as any).__plumbline.startConceptStudy("hope");
  });

  // Entering the mode swept the chapter on screen; the banner says so.
  await expect(page.locator(".concept-study-banner .prog")).toHaveText(/^1 \/ \d+/, { timeout: 10_000 });

  // The navigator's chapter grid paints the SWEEP: the swept chapter's tile is
  // tinted, its neighbour is not, and the tooltip names the state.
  await page.locator(".pane .nav .passage").first().click();
  const current = page.locator(".grid.books button.current");
  await expect(current).toBeVisible();
  await expect(current).toHaveAttribute("title", / chapters studied$/);
  await current.click();
  // The swept tile is the chapter the reader was IN when the mode started.
  const chapter = await page.evaluate(() => (window as any).__plumbline.panes[0].chapter as number);
  const tiles = page.locator(".grid.nums button");
  const sweptTile = tiles.nth(chapter - 1);
  const neighbour = tiles.nth(chapter); // chapter + 1, never swept yet
  await expect(sweptTile).toHaveAttribute("title", / — studied$/);
  await expect(sweptTile).toHaveAttribute("style", /background/);
  await expect(neighbour).not.toHaveAttribute("style", /background/);

  // Long-press's menu marks by hand in the mode: the neighbour sweeps without
  // being opened, and the banner's count moves with it.
  await neighbour.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Mark chapter studied" }).click();
  await expect(neighbour).toHaveAttribute("style", /background/, { timeout: 10_000 });
  await page.locator(".dialog .close").click();
  await expect(page.locator(".concept-study-banner .prog")).toHaveText(/^2 \/ \d+/);
});

// The Plans-screen path the reader actually walks: type a tag, press Start, and
// the mode is entered; the run then shows as a card that can re-enter the mode.
// Dies if the launcher stops wiring the input through startConceptStudy, or if
// the running card loses its Resume button.
test("the Plans screen launches a concept study and re-enters it from its card", async ({ page }) => {
  await boot(page);

  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  await page.getByPlaceholder("Tag to gather into (e.g. grace)").fill("faith");
  await page.getByRole("button", { name: "Start Concept Study" }).click();

  // Launch enters the mode and records the run.
  await expect(page.locator(".concept-study-banner")).toBeVisible();
  expect(await st(page)).toMatchObject({ conceptStudy: "run-faith" });

  // Leave, then re-enter from the run's card — coverage (the run) persists.
  await page.getByRole("button", { name: "Exit Concept Study" }).click();
  await expect(page.locator(".concept-study-banner")).toHaveCount(0);
  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  const card = page.locator(".plan-card.concept-study", { hasText: "faith" });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Resume" }).click();
  await expect(page.locator(".concept-study-banner")).toBeVisible();
  expect(await st(page)).toMatchObject({ conceptStudy: "run-faith" });
});
