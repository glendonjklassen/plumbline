import { expect, test, type Page } from "@playwright/test";

// The concept study is a non-linear sweep with its own reader mode. Three properties,
// each a way the feature fails quietly:
//
//   1. A verse tap tags under the run's preset instead of opening word study. Dies if
//      the tap path ignores `inConceptStudy`.
//   2. The reading tracker is suspended, so skimming credits no dwell to the reading
//      map that plans derive progress from. Dies if Shell's tracker `target` drops
//      the `inConceptStudy` guard.
//   3. Leaving restores word-study taps and the tracker, and the tag survives. Dies
//      if exitConceptStudy does not clear config.conceptStudy.

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

/** How many verses the tag holds, straight from the engine: the sweep's yield. */
const tagCount = (page: Page, tag: string): Promise<number> =>
  page.evaluate(async (t) => {
    const tags = await (window as any).__plumbline.engine.tags();
    return tags?.tags?.find((x: any) => x.name === t)?.members?.length ?? 0;
  }, tag);

test("a concept study tags on tap, suspends the tracker, and its tag outlives the run", async ({ page }) => {
  await boot(page);

  // Enter the mode through the engine; the Plans-panel launcher has its own test.
  await page.evaluate(async () => {
    await (window as any).__plumbline.startConceptStudy("grace");
  });

  // (2) In the mode the tracker's target is null, so it reports nothing.
  expect(await st(page)).toMatchObject({ conceptStudy: "run-grace" });
  await expect(page.locator(".concept-study-banner")).toBeVisible();

  // (1) A verse tap opens the app's ConfirmDialog, not word study; its verb button
  //     names the act ("Tag …"), which is how the confirm is accepted below.
  const canvas = page.locator(".pane canvas").first();
  const box = (await canvas.boundingBox())!;
  await canvas.click({ position: { x: box.width / 2, y: 40 } });

  const confirm = page.getByRole("button", { name: /^Tag / });
  await expect(confirm).toBeVisible({ timeout: 10_000 });
  await confirm.click();

  // Asserted as "no panel at all" rather than "not word study": a fallen-through tap
  // opens whatever that release's tap answer is, so a one-kind check would sleep
  // through the regression.
  expect((await st(page)).panelKind).toBeNull();

  await expect
    .poll(() => tagCount(page, "grace"), { message: "the tap did not file a verse under the preset tag", timeout: 10_000 })
    .toBeGreaterThan(0);

  // (3) Leaving clears the config and the banner, and the tag stays.
  await page.getByRole("button", { name: "Exit Concept Study" }).click();
  expect((await st(page)).conceptStudy).toBe("");
  await expect(page.locator(".concept-study-banner")).toHaveCount(0);
  expect(await tagCount(page, "grace")).toBeGreaterThan(0);
});

// A touch tap is delivered twice: pointerup, then a synthesized `click` a few ms
// later. Unswallowed, the second delivery either runs the tap again or lands on the
// confirm's backdrop and answers "no" to a question the reader never saw. Dies if
// ReaderPane stops setting `suppressClick` on the touch-tap path, or if the confirm
// backdrop goes back to dismissing on click.
test.describe("touch", () => {
  test.use({ hasTouch: true });

  test("a touch tap asks exactly once, and the confirm survives its ghost click", async ({ page }) => {
    await boot(page);
    await page.evaluate(async () => {
      await (window as any).__plumbline.startConceptStudy("mercy");
    });
    // Warm the plans read: only on the warm path does the confirm open fast enough
    // for the ghost click to land on its backdrop. Cold, the RPC outruns it.
    await page.evaluate(() => (window as any).__plumbline.fetchQ("plans", ""));
    // A double-delivered tap calls the handler twice, wherever the ghost click lands.
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

    // The confirm appears and stays, past any ghost-click window.
    const confirm = page.getByRole("button", { name: /^Tag / });
    await expect(confirm).toBeVisible({ timeout: 10_000 });
    await page.waitForTimeout(400);
    await expect(confirm).toBeVisible();

    // One tap, one ask.
    expect(await page.evaluate(() => (window as any).__tagCalls)).toBe(1);

    // Accepting files the verse, and no second confirmation surfaces.
    await confirm.click();
    await expect(page.locator('[data-surface="confirm"]')).toHaveCount(0);
    await expect
      .poll(() => tagCount(page, "mercy"), { message: "the touch tap did not file the verse", timeout: 10_000 })
      .toBe(1);
    await page.waitForTimeout(300);
    await expect(page.locator('[data-surface="confirm"]')).toHaveCount(0);
  });
});

// In the mode the navigator paints swept coverage, not the reading map, whose glow
// cannot move while the tracker is suspended. Dies if the banner loses its count, if
// `swept` falls off the plans wire, or if BookNav paints the reading map here again.
test("the mode shows sweep progress — the banner counts and the navigator paints coverage", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    await (window as any).__plumbline.startConceptStudy("hope");
  });

  // Entering the mode swept the chapter on screen; the banner says so.
  await expect(page.locator(".concept-study-banner .prog")).toHaveText(/^1 \/ \d+/, { timeout: 10_000 });

  // The swept chapter's tile is tinted, its neighbour is not, and the title names it.
  await page.locator(".pane .nav .passage").first().click();
  const current = page.locator(".grid.books button.current");
  await expect(current).toBeVisible();
  await expect(current).toHaveAttribute("title", / chapters studied$/);
  await current.click();
  // The swept tile is the chapter the reader was in when the mode started.
  const chapter = await page.evaluate(() => (window as any).__plumbline.panes[0].chapter as number);
  const tiles = page.locator(".grid.nums button");
  const sweptTile = tiles.nth(chapter - 1);
  const neighbour = tiles.nth(chapter); // chapter + 1, never swept yet
  await expect(sweptTile).toHaveAttribute("title", / — studied$/);
  await expect(sweptTile).toHaveAttribute("style", /background/);
  await expect(neighbour).not.toHaveAttribute("style", /background/);

  // The tile menu marks by hand: the neighbour sweeps without being opened, and the
  // banner's count moves with it.
  await neighbour.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Mark chapter studied" }).click();
  await expect(neighbour).toHaveAttribute("style", /background/, { timeout: 10_000 });
  await page.locator(".dialog .close").click();
  await expect(page.locator(".concept-study-banner .prog")).toHaveText(/^2 \/ \d+/);
});

// The Plans-screen path a reader walks. Dies if the launcher stops wiring its input
// through startConceptStudy, or if the running card loses its Resume button.
test("the Plans screen launches a concept study and re-enters it from its card", async ({ page }) => {
  await boot(page);

  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  await page.getByPlaceholder("Tag to add verses to (e.g. grace)").fill("faith");
  await page.getByRole("button", { name: "Start Concept Study" }).click();

  await expect(page.locator(".concept-study-banner")).toBeVisible();
  expect(await st(page)).toMatchObject({ conceptStudy: "run-faith" });

  // Leaving and re-entering from the card keeps the run and its coverage.
  await page.getByRole("button", { name: "Exit Concept Study" }).click();
  await expect(page.locator(".concept-study-banner")).toHaveCount(0);
  await page.evaluate(() => (((window as any).__plumbline as any).screen = "plans"));
  const card = page.locator(".plan-card.concept-study", { hasText: "faith" });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Resume" }).click();
  await expect(page.locator(".concept-study-banner")).toBeVisible();
  expect(await st(page)).toMatchObject({ conceptStudy: "run-faith" });
});
