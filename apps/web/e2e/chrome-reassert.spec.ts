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

  // What the UA does to us, reproduced exactly: a light cream bar and a light
  // `color-scheme`, with NOTHING in the app's state moved. (This was the
  // manifest's own `theme_color` until 2026-08-28; the manifest no longer
  // declares one — see manifest.spec.ts — but a UA re-deriving from ANY source
  // is the same event from where this page stands.) No $effect can re-run — none of their dependencies changed — so only
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

// E — the home indicator's bar. Chrome draws Android's navigation bar transparent
// only while the window is edge-to-edge, which is what index.html's
// `viewport-fit=cover` asks for; Blink reports that ask once, on change, and a
// cold launch can lose it — white bar under a Nord page until the app is switched
// away from and back (maintainer, 2026-09-23). `Session.#reclaimEdges` re-sends
// the opt-in at every re-assert moment by flipping the value away and back, and
// ONLY while the bottom inset reads 0px: a page already under the home indicator
// has one, and flipping its opt-in would drop it out of edge-to-edge for a frame.
// The bottom ALONE: v0.74.0 gated on all four insets, and the launch it was for
// turned out to be half extended — under the status bar, not under the home
// indicator (the maintainer's screenshot, 2026-09-23) — so the gate never opened.
//
// CAN FAIL: nothing else in apps/web/src writes the viewport tag after load —
// before the fix the observer below records no mutation at all and the first
// assertion fails; a version that skipped the inset gate would mutate under the
// full set of insets and fail the second half; and the v0.74.0 gate fails the
// third, where a status-bar inset is present and the bottom is not.
test("the edge-to-edge opt-in is re-sent while the home indicator has no inset, and left alone when it has one", async ({
  page,
}) => {
  await bootDark(page);
  await chromeIsTheme(page);

  // Record every write to the viewport tag from here on by the value it
  // REPLACED — the records arrive batched, after both writes, so the live
  // attribute would read the same final value twice. The value it is left at
  // is checked separately: the flip must END on `cover` or the notch is lost
  // for real.
  const arm = () =>
    page.evaluate(() => {
      const meta = document.querySelector('meta[name="viewport"]')!;
      const w = window as any;
      w.__viewportWrites = [] as (string | null)[];
      w.__viewportObserver?.disconnect();
      w.__viewportObserver = new MutationObserver((records) => {
        for (const r of records) if (r.attributeName === "content") w.__viewportWrites.push(r.oldValue);
      });
      w.__viewportObserver.observe(meta, { attributes: true, attributeOldValue: true });
    });
  const writes = () => page.evaluate(() => (window as any).__viewportWrites as (string | null)[]);
  const viewport = () =>
    page.evaluate(() => document.querySelector('meta[name="viewport"]')!.getAttribute("content"));
  const COVER = "width=device-width, initial-scale=1.0, viewport-fit=cover";
  const AUTO = "width=device-width, initial-scale=1.0, viewport-fit=auto";

  // No inset (a headless desktop, and exactly the lost-opt-in state on a phone):
  // a return to the foreground re-sends the ask — away, then back.
  await arm();
  await page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
  await expect.poll(writes, { message: "the opt-in is flipped away and back" }).toEqual([COVER, AUTO]);
  expect(await viewport(), "and ends where index.html put it").toBe(COVER);
  // And the colour half of the same moment still runs, in the same call.
  await expect.poll(() => chromeState(page)).toEqual(
    await page.evaluate(() => {
      const s = (window as any).__plumbline;
      return {
        tags: [(s.chrome.color as string).toLowerCase(), (s.chrome.color as string).toLowerCase()],
        scheme: s.chrome.dark ? "dark" : "light",
      };
    }),
  );

  // Under a notch and a home indicator (safe-area.spec.ts's way of having one),
  // the page IS edge-to-edge and the tag is not to be touched.
  await page.evaluate(() => {
    const r = document.documentElement.style;
    r.setProperty("--safeTop", "44px");
    r.setProperty("--safeBottom", "34px");
  });
  await arm();
  await page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
  await page.evaluate(() => dispatchEvent(new Event("pageshow")));
  // The theme-colour re-assert is synchronous in the same handler, so once it has
  // landed the viewport half has had its chance.
  await expect.poll(() => chromeState(page)).toBeTruthy();
  expect(await writes(), "a home-indicator inset means the page is already under it").toEqual([]);

  // Half extended — the launch in the screenshot: under the status bar, with
  // nothing under the home indicator. This is the state the whole thing exists
  // for, and the one a gate on every inset missed.
  await page.evaluate(() => document.documentElement.style.removeProperty("--safeBottom"));
  await arm();
  await page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
  await expect.poll(writes, { message: "a status-bar inset alone does not stand the re-send down" }).toEqual([
    COVER,
    AUTO,
  ]);
  expect(await viewport()).toBe(COVER);
  await page.evaluate(() => document.documentElement.style.removeProperty("--safeTop"));
});
