import { expect, test, type Page } from "@playwright/test";

// The reader's text size moves the CHROME, not only the scripture.
//
// Settings ▸ Text size has always resized the page and left everything around it
// at the sizes it was drawn at: a 13px menu, a 12px label and a 14px button
// around 40px scripture. The one exception was the study panel, which has
// multiplied by `--uiScale` since 2026-07-25 — so the variable existed, on one
// surface, and thirteen others did not know about it. It is published on `:root`
// now (lib/uiScale.ts, declared in app.css) and every chrome font-size in the
// shell is `calc(Npx * var(--uiScale, 1))`.
//
// Two inputs, and the tests below drive them separately:
//   * the reader's own setting (`bodySize`, 18px is 1);
//   * the BROWSER's default font size, which a chrome written entirely in `px`
//     cannot otherwise hear at all.
//
// The second one has no Playwright API, and it needs none: a browser font
// preference IS the used font-size of the root element, and nothing in this app
// sets that — so writing it directly is the same input the preference would
// give. (The same reasoning as the safe-area insets in app.css, which are
// variables precisely so a headless browser can be given a notch.)

const DESKTOP = { width: 1100, height: 800 };

async function boot(page: Page, vp = DESKTOP): Promise<void> {
  await page.setViewportSize(vp);
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** `--uiScale` as the page has computed it. */
async function scale(page: Page): Promise<number> {
  return await page.evaluate(() =>
    Number(getComputedStyle(document.documentElement).getPropertyValue("--uiScale").trim()),
  );
}

async function fontPx(page: Page, sel: string): Promise<number> {
  return await page.locator(sel).first().evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
}

// Mutation: drop `use:uiScale={readerScale}` from the probe in Shell.svelte →
//   'Error: the reader turned the text up and --uiScale did not move
//    expect(received).toBeCloseTo(expected)  Expected: 2  Received: 1'.
test("the reader's text size is published on :root", async ({ page }) => {
  await boot(page);
  // 18px is the size the chrome was drawn at, so the default is exactly 1 and
  // nothing on screen moves for a reader who never touches the setting.
  expect(await scale(page)).toBeCloseTo(1, 3);

  await page.evaluate(() => (window as any).__plumbline.setZoom(36));
  await expect.poll(() => scale(page), { timeout: 5_000 }).toBeCloseTo(2, 3);

  await page.evaluate(() => (window as any).__plumbline.setZoom(9));
  // setZoom clamps at 12, so the smallest the chrome ever gets is 12/18.
  await expect.poll(() => scale(page), { timeout: 5_000 }).toBeCloseTo(12 / 18, 2);
});

/**
 * A sample from every corner of the chrome: the header, the destination bar's
 * labels, a dialog, a sheet and the study panel. Named individually rather than
 * swept, because what is being asserted is that each of these files was actually
 * converted — a sweep over "every element with a font-size" would pass on the
 * ones already scaled and say nothing about the ones that were missed.
 */
const CHROME: { name: string; open?: string; sel: string; drawnAt: number }[] = [
  { name: "the app's name in the header", sel: "header .title", drawnAt: 18 },
  { name: "the passage in the header", sel: "header .subtitle", drawnAt: 16 },
  { name: "the header's Share button", sel: "header .share-first", drawnAt: 15 },
  { name: "a destination in the top bar", sel: "header .browse button", drawnAt: 16 },
  { name: "the settings dialog's heading", open: `s.showSettings = true`, sel: '[data-surface="settings"] h2', drawnAt: 17 },
  { name: "a settings label", open: `s.showSettings = true`, sel: '[data-surface="settings"] .label', drawnAt: 12 },
  { name: "the tag sheet's heading", open: `s.tagPickFor = "John 3:16"`, sel: '[data-surface="tag picker"] h2', drawnAt: 15 },
  { name: "the verse context menu", open: `s.contextMenu = { x: 40, y: 120, refKey: "John 3:16" }`, sel: ".menu > button", drawnAt: 14.5 },
  { name: "the study panel", open: `s.panel = { kind: "guide" }`, sel: '[data-surface="study panel"]', drawnAt: 16 },
];

// Mutation: revert ONE of them — e.g. `.title { font-size: 18px }` in
//   Shell.svelte → 'Error: these parts of the chrome ignore the reader's text
//   size:  the app's name in the header — drawn at 18px, still 18px at a scale
//   of 2'. The failure names the offender, so it is also the punch list.
test("the whole chrome follows the reader's text size", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => (window as any).__plumbline.setZoom(36));
  await expect.poll(() => scale(page), { timeout: 5_000 }).toBeCloseTo(2, 3);

  const stuck: string[] = [];
  for (const c of CHROME) {
    await page.evaluate(() => (window as any).__plumbline.dismissTransient());
    if (c.open) await page.evaluate(`(() => { const s = window.__plumbline; ${c.open}; })()`);
    const el = page.locator(c.sel).first();
    await expect(el, `${c.name} should be on screen`).toBeVisible({ timeout: 20_000 });
    const got = await fontPx(page, c.sel);
    // Half a pixel of slack for the three-decimal scale and sub-pixel rounding.
    if (Math.abs(got - c.drawnAt * 2) > 0.5) {
      stuck.push(`${c.name} — drawn at ${c.drawnAt}px, ${got}px at a scale of 2`);
    }
  }
  await page.evaluate(() => (window as any).__plumbline.dismissTransient());

  expect(stuck, `these parts of the chrome ignore the reader's text size:\n  ${stuck.join("\n  ")}`).toEqual(
    [],
  );
});

// THE READER IS NOT THE CHROME. The scripture is painted into a canvas from the
// engine's own font size; `--uiScale` must not reach it, or a reader who turned
// the text up would get it twice — once from the engine and once from CSS.
//
// Driven by setting `--uiScale` directly rather than by moving the setting,
// because moving the setting changes BOTH inputs and could not tell the two
// apart. This is the isolated question: does the scale, on its own, touch the
// page?
//
// Mutation: add `body { font-size: calc(16px * var(--uiScale, 1)); }` to app.css
//   — i.e. scale the chrome the "convert the root" way instead — →
//   'Error: the chrome's text scale reached the reading pane
//    expect(received).toEqual(expected) // deep equality
//    - Object {"font": "16px", "w": 1100}
//    + Object {"font": "48px", "w": 1100}'.
//
// HEIGHT IS NOT MEASURED, and deliberately: the header itself gets taller when
// its text grows, so the pane below it is shorter — that is the change working,
// not the scale leaking into the page. Width and the canvas's own inherited
// font-size are the two that must not move.
test("the chrome's text scale does not touch the reader", async ({ page }) => {
  await boot(page);
  const measure = () =>
    page.locator(".pane canvas").first().evaluate((el) => ({
      w: Math.round(el.getBoundingClientRect().width),
      font: getComputedStyle(el).fontSize,
    }));
  const before = await measure();
  expect(before.w).toBeGreaterThan(100);

  await page.evaluate(() => document.documentElement.style.setProperty("--uiScale", "3"));
  // The chrome really did react, or the rest of this proves nothing.
  await expect.poll(() => fontPx(page, "header .title"), { timeout: 5_000 }).toBeCloseTo(54, 0);

  expect(await measure(), "the chrome's text scale reached the reading pane").toEqual(before);
  // And the chapter is still the same chapter, laid out the same way.
  await expect(page.locator(".pane .mirror p")).toHaveCount(36);
});

// The other half of the setting nobody can hear: a reader who has told their
// BROWSER they want 20px text has told every site, and a chrome written in `px`
// answers 13px anyway. `--uiScale` carries it because lib/uiScale.ts measures a
// one-rem box, and a ResizeObserver on that box means the answer follows a
// preference changed while the app is open — which fires no event a script can
// otherwise hear.
//
// Mutation: in lib/uiScale.ts replace
//   `const rootPx = node.getBoundingClientRect().width || CSS_DEFAULT_PX;`
//   with `const rootPx = CSS_DEFAULT_PX;` →
//   'Error: the chrome ignores the browser's own text size
//    expect(received).toBeCloseTo(expected)  Expected: 1.5  Received: 1'.
test("the chrome follows the browser's own text size", async ({ page }) => {
  await boot(page);
  expect(await scale(page)).toBeCloseTo(1, 3);
  const drawnAt = await fontPx(page, "header .title");
  expect(drawnAt).toBeCloseTo(18, 0);

  // A browser font preference IS the root element's font-size. 24px is the
  // "Large" setting in Chrome's own font-size menu.
  await page.evaluate(() => (document.documentElement.style.fontSize = "24px"));
  await expect
    .poll(() => scale(page), { timeout: 5_000 })
    .toBeCloseTo(1.5, 3);
  expect(
    await fontPx(page, "header .title"),
    "the chrome ignores the browser's own text size",
  ).toBeCloseTo(27, 0);

  // And the two inputs multiply: a reader who has set both wants both.
  await page.evaluate(() => (window as any).__plumbline.setZoom(27));
  await expect.poll(() => scale(page), { timeout: 5_000 }).toBeCloseTo(2.25, 3);
});

// THE NARROWEST PHONE, AT THE LARGEST TEXT — the case that made the header's old
// `flex-wrap: nowrap` untenable. nowrap does not mean "one row"; it means the row
// overflows and what runs off the end is the ≡, which is the only way to
// Settings. That already happened below ~340px at the default size, and once the
// chrome follows the reader's text size it happens on any phone.
//
// Mutation: put `flex-wrap: nowrap;` back in the `max-width: 700px` header rule
//   in Shell.svelte → 'Error: the ≡ menu has been pushed off the side of the
//   screen  expect(received).toBeLessThanOrEqual(expected)  Expected: <= 321
//   Received: <the row's min-content width, comfortably past 400>'.
test("the menu stays on screen on a narrow phone at a large text size", async ({ page }) => {
  await boot(page, { width: 320, height: 700 });
  await page.evaluate(() => (window as any).__plumbline.setZoom(40));
  await expect.poll(() => scale(page), { timeout: 5_000 }).toBeCloseTo(40 / 18, 2);

  const menu = page.locator('header [aria-label="Menu"]');
  await expect(menu).toBeVisible();
  const right = await menu.evaluate((el) => el.getBoundingClientRect().right);
  expect(Math.round(right), "the ≡ menu has been pushed off the side of the screen").toBeLessThanOrEqual(
    321,
  );

  // Inside the viewport is necessary, not sufficient: it has to be tappable,
  // which is the only proof nothing is sitting on top of it.
  await menu.click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
});
