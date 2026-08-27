import { expect, test, type Page } from "@playwright/test";

// The status bar follows the READER'S theme, whatever the phone's own scheme.
// index.html ships two media-scoped theme-color tags (the pre-script paint —
// manifest.spec.ts pins those); once the palette is known the app rewrites
// them. It rewrote only the FIRST, the light-scoped one — so a dark-mode phone,
// whose UA reads the SECOND, kept Theme::Dark's paper under a light reader
// theme. Chrome then drew light status-bar icons over the cream the page paints
// beneath its transparent bar: the clock and battery washed out (maintainer,
// 2026-08-25). Every tag has to carry the resolved paper.

test("every theme-color tag carries the reader's paper, on a dark-mode device too", async ({ page }) => {
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
  const paper = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.theme = "light";
    s.applyTheme();
    return s.palette.paper as string;
  });
  expect(paper.toLowerCase()).toBe("#fcf9f4");
  const contents = await page.evaluate(() =>
    [...document.querySelectorAll('meta[name="theme-color"]')].map((m) => m.getAttribute("content")),
  );
  expect(contents.length, "the media-scoped pair is still there to be rewritten").toBe(2);
  for (const c of contents) expect(c?.toLowerCase(), "a tag the UA may be reading was left stale").toBe(paper.toLowerCase());
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
// Every assertion reads the COMPUTED background off the live element rather than
// comparing two literals. The point is that the chrome agrees with what is
// actually painted under it, so a test that pinned "#fcf9f4" on both sides would
// keep passing if the screen's own paper moved.

async function bootDark(page: Page): Promise<void> {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  // A DARK reader theme, chosen by hand — the case where the theme's paper and
  // these screens' paper disagree. Theme::Dark's paper is #1f1b16.
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.theme = "dark";
    s.applyTheme();
  });
}

/** Every theme-color tag's content, lowercased — the UA reads whichever its
 *  media matches, so they are only ever checked together. */
const chrome = (page: Page) =>
  page.evaluate(() =>
    [...document.querySelectorAll('meta[name="theme-color"]')].map((m) =>
      (m.getAttribute("content") ?? "").toLowerCase(),
    ),
  );

/** An element's computed background as `#rrggbb`, to compare with a tag. */
const paintedUnderTheBar = (page: Page, sel: string) =>
  page.evaluate((q) => {
    const [r, g, b] = getComputedStyle(document.querySelector(q)!)
      .backgroundColor.match(/\d+/g)!
      .map(Number);
    return "#" + [r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("");
  }, sel);

/** The chrome is back on the reader's OWN theme — the ordinary case, and what
 *  every one of these screens has to restore when it closes. */
async function chromeIsTheme(page: Page): Promise<void> {
  const paper = await page.evaluate(() =>
    ((window as any).__plumbline.palette.paper as string).toLowerCase(),
  );
  for (const c of await chrome(page))
    expect(c, "the sunlight override outlived the screen that wanted it").toBe(paper);
  expect(await page.evaluate(() => document.documentElement.style.colorScheme)).toBe("dark");
}

/** The tags AND `color-scheme` both follow `sel`'s own paper. */
async function chromeFollows(page: Page, sel: string, scheme: "light" | "dark"): Promise<void> {
  const painted = await paintedUnderTheBar(page, sel);
  const tags = await chrome(page);
  expect(tags.length, "the media-scoped pair is still there to be rewritten").toBe(2);
  for (const c of tags)
    expect(c, `the clock is drawn onto ${sel}'s paper, so it has to be tinted for it`).toBe(painted);
  expect(
    await page.evaluate(() => document.documentElement.style.colorScheme),
    "the scrollbars and any UA control on this screen belong to that paper too",
  ).toBe(scheme);
}

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
