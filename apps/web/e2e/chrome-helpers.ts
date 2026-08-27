import { expect, type Page } from "@playwright/test";

/**
 * The status-bar chrome, as both specs that ask about it need to see it —
 * theme-color.spec.ts asks what the chrome SAYS, chrome-reassert.spec.ts asks
 * whether it is still saying it. One copy, because two would drift and the
 * drift is the whole bug family these cover.
 *
 * Every assertion here reads the COMPUTED background off a live element rather
 * than comparing two literals. The claim is that the chrome agrees with what is
 * actually painted under it, and a test pinning the same hex on both sides
 * would keep passing after the surface moved.
 */

/** An established profile on the read screen, dark device, dark reader theme —
 *  the case where the theme's own surfaces and the two fixed-light screens
 *  disagree. Theme::Dark's paneNavBg is #262019. */
export async function bootDark(page: Page): Promise<void> {
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
  // The CHOICE, and nothing else: the writers are $effects, and a test that
  // called one by hand would be testing a path the app no longer takes.
  await page.evaluate(() => ((window as any).__plumbline.config.theme = "dark"));
  await settled(page);
}

/** Wait until the document actually carries what the session has derived.
 *  Anything that reads a computed background has to be behind this, or it
 *  measures the palette that is on its way out. Stated against the session's
 *  own values rather than a hex, so it cannot go stale. */
export async function settled(page: Page): Promise<void> {
  await expect
    .poll(() =>
      page.evaluate(() => {
        const s = (window as any).__plumbline;
        const root = document.documentElement;
        return (
          root.style.getPropertyValue("--paneNavBg") === s.palette.paneNavBg &&
          root.style.colorScheme === (s.chrome.dark ? "dark" : "light")
        );
      }),
    )
    .toBe(true);
}

/** Every theme-color tag's content, lowercased — the UA reads whichever its
 *  media matches, so they are only ever checked together. */
export const chrome = (page: Page) =>
  page.evaluate(() =>
    [...document.querySelectorAll('meta[name="theme-color"]')].map((m) =>
      (m.getAttribute("content") ?? "").toLowerCase(),
    ),
  );

/** An element's computed background as `#rrggbb`, to compare with a tag. */
export const paintedUnderTheBar = (page: Page, sel: string) =>
  page.evaluate((q) => {
    const [r, g, b] = getComputedStyle(document.querySelector(q)!)
      .backgroundColor.match(/\d+/g)!
      .map(Number);
    return "#" + [r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("");
  }, sel);

/** The tags AND `color-scheme` in one read. They are written together and are
 *  only ever true together — a bar tinted for one surface with icons chosen for
 *  another IS the washout — so they are only ever read together. */
export const chromeState = (page: Page) =>
  page.evaluate(() => ({
    tags: [...document.querySelectorAll('meta[name="theme-color"]')].map((m) =>
      (m.getAttribute("content") ?? "").toLowerCase(),
    ),
    scheme: document.documentElement.style.colorScheme,
  }));

/** The tags AND `color-scheme` both follow `sel`'s own paper.
 *
 *  The two-element array is load-bearing: the media-scoped pair must still be
 *  there to rewrite, and both members must carry the same string.
 *
 *  Polled, because the writers are $effects: whatever moved the state — a
 *  click, an assignment — does not itself paint. A poll with no fixed ceiling
 *  is the honest way to wait for a flush. */
export async function chromeFollows(page: Page, sel: string, scheme: "light" | "dark"): Promise<void> {
  const painted = await paintedUnderTheBar(page, sel);
  await expect
    .poll(() => chromeState(page), {
      message: `the clock is drawn onto ${sel}'s paper, so the tags and the UA controls belong to it too`,
    })
    .toEqual({ tags: [painted, painted], scheme });
}

/** The chrome is back on the reader's OWN theme — the ordinary case, and what
 *  every fullscreen screen has to restore when it closes. The comparison is
 *  against the live HEADER, because that is the surface under the bar once
 *  those screens are gone (`--paneNavBg`, which is also what Android paints
 *  behind its system bars). */
export const chromeIsTheme = (page: Page) => chromeFollows(page, "header", "dark");
