import { expect, test, type Page } from "@playwright/test";

// The foldable band: wide enough for the desktop chrome, not wide enough for three of
// anything. The web decides its layout by width, and the two breakpoints disagreed —
// `s.narrow` (one pane, bottom bar) flipped at 700px while the study panel stayed a
// bottom sheet to 900, so every viewport from 701 to 900 got the desktop chrome with a
// study surface covering the reader. An unfolded Pixel Fold browser is ~841 CSS px and
// landed exactly there.
//
// The band is pinned from both ends: 841 is a real unfolded Pixel Fold, 390 the same
// device folded. The folded case is here because "make the sidebar unconditional" would
// pass the wide case alone, and would put a 380px sidebar on a phone.

const FOLD_OPEN = { width: 841, height: 763 };
const FOLD_SHUT = { width: 390, height: 763 };

async function boot(page: Page, vp: { width: number; height: number }): Promise<void> {
  await page.setViewportSize(vp);
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Raise the study panel through session state: this is about the layout, not about
 *  which tap happens to open it. */
async function openStudy(page: Page): Promise<void> {
  await page.evaluate(() => {
    (window as any).__plumbline.panel = { kind: "notesBrowser" };
  });
  await expect(page.locator('[data-surface="study panel"]')).toBeVisible();
}

test("unfolded: study sits beside the text, not over it", async ({ page }) => {
  await boot(page, FOLD_OPEN);
  await openStudy(page);

  const panel = await page.locator('[data-surface="study panel"]').boundingBox();
  const reader = await page.locator(".pane").first().boundingBox();
  expect(panel).not.toBeNull();
  expect(reader).not.toBeNull();

  // Beside: the panel begins after the reader ends. A bottom sheet fails this — it
  // spans the full width, so its left edge is 0.
  expect(panel!.x).toBeGreaterThanOrEqual(reader!.x + reader!.width - 1);
  // The 40vw cap exists so scripture keeps the larger share of the window.
  expect(reader!.width).toBeGreaterThan(FOLD_OPEN.width * 0.5);
  expect(panel!.width).toBeLessThanOrEqual(FOLD_OPEN.width * 0.4 + 1);

  // Full height, like a sidebar — not a 62dvh sheet clipped at the bottom.
  expect(panel!.height).toBeGreaterThan(FOLD_OPEN.height * 0.7);
});

test("folded: the same panel is still a bottom sheet", async ({ page }) => {
  await boot(page, FOLD_SHUT);
  await openStudy(page);

  const panel = await page.locator('[data-surface="study panel"]').boundingBox();
  expect(panel!.x).toBeLessThanOrEqual(1);
  expect(panel!.width).toBeGreaterThan(FOLD_SHUT.width - 2);
  // A sheet, so the reader is behind it and the panel does not own the top.
  expect(panel!.y).toBeGreaterThan(FOLD_SHUT.height * 0.2);
});

test("unfolded: two panes fit, three do not", async ({ page }) => {
  await boot(page, FOLD_OPEN);

  // The offer and the rule are the same number: the control is shown iff `addPane`
  // would accept.
  await page.locator('.pane button[title="Split pane"]').first().click();
  await expect(page.locator(".pane")).toHaveCount(2);

  // At 841px a third pane would leave 280px columns, so the control is gone.
  await expect(page.locator('.pane button[title="Split pane"]')).toHaveCount(0);
  // And it is gone because the rule refuses, not merely because it is unrendered.
  await page.evaluate(() => (window as any).__plumbline.addPane(0));
  await expect(page.locator(".pane")).toHaveCount(2);
});

test("a desktop's three-pane config reopens folded on what was being read", async ({ page }) => {
  await boot(page, FOLD_OPEN);
  // Two panes here, and the reader is in the second.
  await page.locator('.pane button[title="Split pane"]').first().click();
  await expect(page.locator(".pane")).toHaveCount(2);
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.activePane = 1;
    s.navigate(1, "Rom", 8);
  });
  await expect(page.locator(".pane")).toHaveCount(2);
  // Give the debounced config write time to land.
  await page.waitForTimeout(1200);

  // Fold the device: one pane, and it must be Romans, not the leftmost John.
  await page.setViewportSize(FOLD_SHUT);
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator(".pane")).toHaveCount(1);
  await expect(page.locator('.pane button[title="Go to… (book · chapter · verse)"]').first()).toContainText("Romans 8");
});

// Folding while RUNNING, which the tests above do not reach: they reload at the new
// width, and the boot path has always enforced `maxPanes`. Nothing enforced it when the
// width changed under a live app, so shutting a foldable kept both panes on a layout
// that assumes one. The language is the other half: the chip that sets a pane's
// language lives on the pane's own strip, which Shell hides under 700px, so a per-pane
// override was left with no control to undo it. Folding hands the pane back to the app
// language, which Settings can reach on a phone.
//
// Fails against that bug: before `#collapseToPhone` the media-query listener only
// assigned `s.narrow`, so both assertions below held their pre-fold values. `lang` is
// planted directly rather than through `setPaneLang` because that call downloads an
// 8 MB German corpus, and the fold response is what is under test, not the download.
test("folding a running app collapses to one pane and hands back its language", async ({ page }) => {
  await boot(page, FOLD_OPEN);
  await page.locator('.pane button[title="Split pane"]').first().click();
  await expect(page.locator(".pane")).toHaveCount(2);

  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.activePane = 1;
    s.navigate(1, "Rom", 8);
    s.panes[1].lang = "de";
  });
  await expect(page.locator(".pane")).toHaveCount(2);

  // Shut it. No reload — this is the live resize.
  await page.setViewportSize(FOLD_SHUT);

  await expect(page.locator(".pane")).toHaveCount(1, { timeout: 30_000 });
  const state = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { panes: s.panes.length, active: s.activePane, lang: s.panes[0].lang ?? null, book: s.panes[0].book };
  });
  expect(state).toEqual({ panes: 1, active: 0, lang: null, book: "Rom" });
});
