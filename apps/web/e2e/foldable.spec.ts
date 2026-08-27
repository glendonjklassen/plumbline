import { expect, test, type Page } from "@playwright/test";

// THE FOLDABLE BAND: wide enough for the desktop shell, not wide enough for
// three of anything.
//
// Android decides its two-pane layout by FOLD POSTURE and the web decides by
// WIDTH, and for a long time the two breakpoints the web used disagreed with
// each other. `s.narrow` (one pane, bottom bar) flipped at 700px, but the study
// panel stayed a bottom sheet all the way to 900 — so every viewport from 701 to
// 900 got the desktop chrome with a study surface that covered the reader. An
// unfolded Pixel Fold browser is ~841 CSS px and landed exactly there: the one
// thing that hardware is for, scripture and study side by side, was the one
// thing the PWA withheld while the native app on the same device gave it.
//
// So this pins the band from both ends. 841 is a real unfolded Pixel Fold; 390 is
// the same device folded, and it is here because the fix could trivially have
// been "make the sidebar unconditional", which would put a 380px sidebar on a
// phone. A test that only checked the wide case would have passed that.
//
// Mutation-tested (2026-08-01): restoring `max-width: 900px` on the panel's
// media query fails `study sits beside the text` ("panel starts at 0, reader ends
// at 841"); dropping the pane cap to the old flat 3 fails `two panes fit, three
// do not`.

const FOLD_OPEN = { width: 841, height: 763 };
const FOLD_SHUT = { width: 390, height: 763 };

async function boot(page: Page, vp: { width: number; height: number }): Promise<void> {
  await page.setViewportSize(vp);
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Raise the study panel through session state — this is about the LAYOUT, not
 *  about which tap happens to open it. */
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

  // BESIDE: the panel begins after the reader ends. This is the assertion the
  // bottom sheet fails — a sheet spans the full width, so its left edge is 0 and
  // it starts far to the LEFT of where the reader ends.
  expect(panel!.x).toBeGreaterThanOrEqual(reader!.x + reader!.width - 1);
  // And the reader keeps the majority of the window: a panel capped at 40vw
  // leaves scripture the larger share, which is the point of the cap.
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

  // The offer and the rule are the same number — the control is shown iff
  // `addPane` would accept. Splitting once is allowed.
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

// FOLDING WHILE RUNNING, which is the case the two tests above do not reach:
// they RELOAD at the new width, and the boot path has enforced `maxPanes` since
// it was written. Nothing enforced it when the width changed under a live app,
// so shutting a foldable kept both panes on a layout that assumes one.
//
// The language is the half that stranded the maintainer (2026-08-26): they
// opened the fold, split, switched the new pane to German, and shut it — and the
// passage was "basically stuck on German". The chip that sets a pane's language
// lives on the pane's own strip, and Shell hides `.pane > .nav` under 700px, so
// the override had no control left to undo it. Folding now hands the pane back
// to the app language, which is the one a phone can actually reach (Settings).
//
// FAILS against the bug it describes: before `#collapseToPhone`, the media-query
// listener only assigned `s.narrow`, so both assertions below held their
// pre-fold values. `lang` is planted directly rather than through
// `setPaneLang`, deliberately — that call downloads an 8 MB German corpus, and
// what is under test is the shell's response to the fold, not the download.
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
