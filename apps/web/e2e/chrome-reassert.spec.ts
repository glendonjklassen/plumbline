import { expect, test } from "@playwright/test";

import { bootDark, chromeFollows, chromeIsTheme, chromeState, settled } from "./chrome-helpers";

/**
 * THE STATUS BAR WASHES OUT, take four.
 *
 * The first three fixes (7dc5f72, v0.61.1, v0.61.4) each moved WHICH tag the
 * app wrote and never WHEN it wrote it or WHAT it wrote it from, and the report
 * came back every time: "I think it's fixed, use the app like normal, then
 * something happens — maybe I'm showing someone something — and I get this
 * persistent washed-out top" (maintainer, 2026-08-27).
 *
 * The mechanism is now one derived value (`Session.chrome`), one writer
 * (`applyChrome`), and an enumerable list of moments where the answer has to be
 * re-asserted because a UA can have replaced it with nothing in our state
 * moving. This file covers that list and the two ways the value itself could
 * still be wrong. theme-color.spec.ts covers what the value SAYS.
 *
 * None of these are mutation-tested — breaking the fix to watch them go red
 * costs a rebuild for less than the reasoning is worth (CLAUDE.md, maintainer
 * 2026-08-26). Each carries its can-fail argument instead.
 */

// A — the re-assert list itself. A real bfcache restore and a real foldable
// activity re-creation are not reachable from Playwright; the contract this
// tests is the one that answers them, which is that each named moment puts the
// chrome back. The resize IS real.
//
// CAN FAIL: before the fix, `addEventListener("resize", …)` appeared nowhere in
// apps/web/src, there was no pageshow listener at all, and the single
// visibilitychange listener flushed the session on HIDDEN and did nothing on
// visible. Nothing else observes these events, so nothing would rewrite the
// tags — the deranged values simply stay and every poll below times out.
test("the chrome is re-asserted at each moment a UA can have re-derived it", async ({ page }) => {
  await bootDark(page);
  await chromeIsTheme(page);

  const want = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return {
      tags: [(s.chrome.color as string).toLowerCase(), (s.chrome.color as string).toLowerCase()],
      scheme: (s.chrome.dark ? "dark" : "light") as "dark" | "light",
    };
  });

  // What the UA does to us, reproduced exactly: the manifest's static, light-only
  // `theme_color` and a light `color-scheme`, with NOTHING in the app's state
  // moved. No $effect can re-run — none of their dependencies changed — so only
  // a listener on the event itself can put this right.
  const derange = () =>
    page.evaluate(() => {
      for (const m of document.querySelectorAll('meta[name="theme-color"]'))
        m.setAttribute("content", "#fcf9f4");
      document.documentElement.style.colorScheme = "light";
    });

  // 0. THE CONTROL, and what keeps the three below from passing vacuously:
  // deranged and left alone, the chrome STAYS deranged. Nothing in this app
  // repaints it on a timer or a frame — only a state change or one of the named
  // events does — so if this ever goes red, the assertions after it have stopped
  // proving that their event was what put the answer back. The fixed wait is
  // sound here precisely because it is a negative: it is not a budget for
  // something to happen in, it is a span in which nothing may. Comfortably past
  // the resize listener's 200 ms debounce.
  await derange();
  await page.waitForTimeout(600);
  expect(await chromeState(page)).toEqual({ tags: ["#fcf9f4", "#fcf9f4"], scheme: "light" });

  // 1. A bfcache restore — Back into the app from another page.
  await derange();
  await page.evaluate(() => dispatchEvent(new Event("pageshow")));
  await expect.poll(() => chromeState(page), { message: "pageshow" }).toEqual(want);

  // 2. Back to the foreground after the phone was elsewhere.
  await derange();
  await page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
  await expect.poll(() => chromeState(page), { message: "visibilitychange" }).toEqual(want);

  // 3. A REAL resize — the proxy for the fold opening or closing, which on
  // Android re-creates the activity and is where the maintainer's reports keep
  // coming from. Polled and not slept on: the listener is trailing-debounced,
  // and a fixed ceiling here would be a number to tune rather than a claim.
  await derange();
  await page.setViewportSize({ width: 380, height: 800 });
  await expect.poll(() => chromeState(page), { message: "resize" }).toEqual(want);
});

// B — the stale presentation. Present is mounted unconditionally with only its
// template gated on `showPresent`, so its `thread` outlived every close except
// the ✕: a back-peel left a presentation running behind a closed screen.
//
// CAN FAIL: before the fix, reopening Present after a back-peel renders the
// PRESENTATION (`.present.picking` never appears), because `thread` is still
// set — so the first wait below times out. And the fix that looks obvious —
// putting `presentingThread` into Session.TRANSIENT, leaving `thread` alone —
// fails the second assertion instead: the presentation is back on screen in its
// cream while the chrome has returned to the dark theme's polarity, which is
// light icons on a light surface. The washout, manufactured by the repair.
test("Present starts over when it is closed by a back-peel, chrome included", async ({ page }) => {
  await bootDark(page);
  await page.locator(".bottom-nav").getByRole("button", { name: "Preach" }).click();
  await page.locator(".ex-card", { hasText: "Present" }).first().click();
  await expect(page.locator(".present.picking")).toBeVisible();

  // The stock set seeds the Romans Road thread, so there is always one to pick.
  await page.locator(".present .pick").first().click();
  await expect(page.locator(".present.picking")).toBeHidden();
  await chromeFollows(page, ".present", "light");

  // The back-peel: what the phone's Back button and Escape both climb, and what
  // PresentHost's own close() never sees.
  await page.evaluate(() => (window as any).__plumbline.popOneLayer());
  await expect(page.locator(".present")).toBeHidden();
  await chromeIsTheme(page);

  // Reopen by the route a reader has — the card, not a flag poked from the
  // console. The PICKER, not the presentation they walked away from.
  await page.locator(".ex-card", { hasText: "Present" }).first().click();
  await expect(page.locator(".present.picking")).toBeVisible();
  await chromeFollows(page, ".present", "dark");
});

// C — a theme token no palette answers to. `plumbline:themeChoice` is read out
// of localStorage at boot, which is the one input to the theme that nothing in
// this app necessarily wrote, and it used to be copied into the config and
// SAVED unvalidated. A miss then painted an empty palette: the `--*` vars kept
// their previous values, so the page stayed dark, while the chrome read `dark`
// off `{}` and wrote cream with `color-scheme: light`. A dark page under a
// light bar, permanently, with the reader's chosen theme also gone.
//
// The device is LIGHT here and the reader's theme is DARK on purpose: it makes
// the two failure modes separable. Falling back to the device would be light
// chrome (wrong, but self-consistent); painting `{}` would be a dark page with
// light chrome (the washout).
//
// CAN FAIL: before the fix, "sparkle-pony" is adopted into `config.theme` and
// saved, `#palettes["sparkle-pony"]` misses, and `palette` is `{}` — so the
// theme assertion fails on the poisoned value and the chrome assertion fails on
// both halves at once.
test("a theme token the palette table does not carry cannot reach the config or the chrome", async ({ page }) => {
  await bootDark(page);
  await page.emulateMedia({ colorScheme: "light" });
  // Persisted for real, so the reload has a home config to disagree with.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.config.theme = "dark";
    s.flushConfig();
    await s.rpc.flush();
  });

  await page.evaluate(() => localStorage.setItem("plumbline:themeChoice", "sparkle-pony"));
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await settled(page);

  expect(
    await page.evaluate(() => (window as any).__plumbline.config.theme),
    "an unrecognised token was adopted into the config, which makes it permanent",
  ).toBe("dark");
  expect(
    await page.evaluate(() => !!(window as any).__plumbline.palette.paneNavBg),
    "the palette resolved to nothing, so every --* var on <html> is a leftover",
  ).toBe(true);
  await chromeFollows(page, "header", "dark");
  // ...and the poison is gone, because applyTheme writes the resolved choice
  // back. A reader does not have to clear their own storage to recover.
  expect(await page.evaluate(() => localStorage.getItem("plumbline:themeChoice"))).toBe("dark");
});

// D — the pipeline, end to end, with the theme changed while a fixed-light
// screen is up. This is the "I was showing someone something" shape: the reader
// is in Sing or Present when the theme moves under them.
//
// CAN FAIL: before the fix, twice over. `setTheme` painted by calling
// `applyTheme()` by hand, so an assignment to `config.theme` on its own moved
// nothing at all; and `applyChrome` chose its colour with `?:`, so while a
// sunlit screen was up it never READ the palette and the effect that called it
// had dropped the theme from its dependencies — meaning even a hand-called
// `applyTheme` behind Sing would not have brought the new answer out with it.
test("a theme changed behind Sing is on the bar when Sing closes", async ({ page }) => {
  await bootDark(page);
  await page.locator(".bottom-nav").getByRole("button", { name: "Sing" }).click();
  await page.locator(".content button.row").first().click();
  await page.locator("button.sing").click();
  await expect(page.locator(".sing-host")).toBeVisible();
  await chromeFollows(page, ".sing-host", "light");

  // The assignment ALONE — no applyTheme(), no applyChrome(). This is exactly
  // what Settings' setTheme does now.
  await page.evaluate(() => ((window as any).__plumbline.config.theme = "light"));
  await page.keyboard.press("Escape");
  await expect(page.locator(".sing-host")).toBeHidden();
  await settled(page);
  await chromeFollows(page, "header", "light");
});
