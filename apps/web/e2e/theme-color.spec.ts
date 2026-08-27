import { expect, test } from "@playwright/test";

import { bootDark, chrome, chromeFollows, chromeIsTheme } from "./chrome-helpers";

// The status bar follows the READER'S theme, whatever the phone's own scheme.
// index.html ships two media-scoped theme-color tags (the pre-script paint —
// manifest.spec.ts pins those); once the palette is known the app rewrites
// them. It rewrote only the FIRST, the light-scoped one — so a dark-mode phone,
// whose UA reads the SECOND, kept Theme::Dark's paper under a light reader
// theme. Chrome then drew light status-bar icons over the cream the page paints
// beneath its transparent bar: the clock and battery washed out (maintainer,
// 2026-08-25). Every tag has to carry the resolved answer — and the answer is
// the surface UNDER the bar (`--paneNavBg`, the header), not the page's paper.

test("every theme-color tag carries the surface under the bar, on a dark-mode device too", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });

  // A LIGHT reader theme on a DARK device — the mismatch that exposed it.
  //
  // The expected value is `paneNavBg` and not `paper`: the bar names WHAT IS
  // PAINTED UNDER IT, and on the read screen that is the header, which is
  // `--paneNavBg` (#efeae1 in Theme::Light). Android has always painted
  // `palette.paneNavBg` behind its system bars; this is the web catching up.
  //
  // Setting the CHOICE and nothing else is the point: `setTheme` no longer
  // calls a writer by hand, so the chrome has to arrive through the derived
  // pipeline or not at all.
  const bar = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.theme = "light";
    return (s.chrome.color as string).toLowerCase();
  });
  expect(bar, "the bar takes the header's surface, not the page's paper").toBe("#efeae1");
  await expect
    .poll(() => chrome(page), { message: "a tag the UA may be reading was left stale" })
    .toEqual([bar, bar]);
});

// ...and the OTHER half of the same report, which that fix does not reach.
// Present and Sing are `position: fixed` OVER the status bar — they carry
// `--safeTop` as their own padding for exactly that reason — and both are
// deliberately fixed-LIGHT, because they are the screens handed across or held
// up in daylight. So on a dark reader theme the tags still named a dark paper,
// Chrome picked white icons to suit it, and drew a white clock and battery onto
// their cream: washed out again, and only intermittently, because it takes one
// of those two screens (maintainer, 2026-08-26).
//
// The helpers these use live in ./chrome-helpers, shared with chrome-reassert.

test("Sing paints over the status bar, so the chrome follows it and not the theme", async ({ page }) => {
  await bootDark(page);
  await chromeIsTheme(page);

  await page.locator(".bottom-nav").getByRole("button", { name: "Sing" }).click();
  await page.locator(".content button.row").first().click();
  await page.locator("button.sing").click();
  await expect(page.locator(".sing-host")).toBeVisible();
  await chromeFollows(page, ".sing-host", "light");

  // ...and back, when it closes. An override that stuck would wash out the bar
  // on every screen after it.
  await page.keyboard.press("Escape");
  await expect(page.locator(".sing-host")).toBeHidden();
  await chromeIsTheme(page);
});

// Present is TWO surfaces in one element, and only one of them is sunlight:
// `.present.picking` restates the palette ("dark mode was jarringly white"),
// while the presentation itself keeps the fixed paper. Pinning both, because
// `s.showPresent` reads like the whole answer and is not — the first cut of this
// fix used it alone and dragged the chrome to cream behind a themed picker.
test("Present's picker keeps the theme; only the presentation pulls the chrome to sunlight", async ({ page }) => {
  await bootDark(page);

  await page.locator(".bottom-nav").getByRole("button", { name: "Preach" }).click();
  await page.locator(".ex-card", { hasText: "Present" }).first().click();
  await expect(page.locator(".present.picking")).toBeVisible();
  await chromeFollows(page, ".present", "dark");

  // The stock set seeds the Romans Road thread, so there is always one to pick.
  await page.locator(".present .pick").first().click();
  await expect(page.locator(".present.picking")).toBeHidden();
  await chromeFollows(page, ".present", "light");

  // Back to the picker with the screen's own ‹ (Escape is the shell's back-peel
  // and closes Present outright) — the chrome has to come back with it.
  await page.locator(".present .close").click();
  await expect(page.locator(".present.picking")).toBeVisible();
  await chromeFollows(page, ".present", "dark");
});
