import { expect, test, type Page } from "@playwright/test";

// `index.html` asks for `viewport-fit=cover`, so an installed PWA gets the strips
// behind the status bar and the home indicator, and — held sideways — the column
// behind the camera cutout. Only the bottom bar ever inset itself: the header sat
// under the clock, and in landscape the reader's first characters were behind the
// cutout.
//
// No Playwright, CDP or emulation switch can give a page a safe-area inset, so
// `app.css` names the four insets once as custom properties whose values are those
// `env(safe-area-inset-*)`s: the app is unchanged on a square screen, and this file
// can set the properties and watch the chrome move. That proves every surface
// consumes the variables, not that the variables carry the OS's numbers — the
// second is one `env()` per side in one rule, checked by reading it.

const PHONE = { width: 390, height: 844 };

/** Distinct per side, so a rule wired to the wrong inset fails instead of passing. */
const INSET = { top: 44, right: 48, bottom: 34, left: 47 };

async function boot(page: Page): Promise<void> {
  await page.setViewportSize(PHONE);
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

async function notch(page: Page, on: boolean): Promise<void> {
  await page.evaluate(
    ({ on, i }) => {
      const r = document.documentElement.style;
      for (const [name, px] of [
        ["--safeTop", i.top],
        ["--safeRight", i.right],
        ["--safeBottom", i.bottom],
        ["--safeLeft", i.left],
      ] as const) {
        if (on) r.setProperty(name, `${px}px`);
        else r.removeProperty(name);
      }
    },
    { on, i: INSET },
  );
}

/** Computed padding, in px, as a number. */
async function pad(page: Page, selector: string, side: string): Promise<number> {
  return await page.locator(selector).evaluate(
    (el, side) => parseFloat(getComputedStyle(el).getPropertyValue(`padding-${side}`)),
    side,
  );
}

/** The frame's FOOT: the strip the frame paints under the home indicator where no
 *  destination bar carries it (Shell.svelte `.frame::after`). A pseudo-element has
 *  no locator, so it is read by computed style — `display` says whether it exists
 *  at that width at all, and the background comes back as `#rrggbb` to compare
 *  with the palette. */
async function foot(page: Page): Promise<{ display: string; height: string; bg: string }> {
  return await page.evaluate(() => {
    const cs = getComputedStyle(document.querySelector(".frame")!, "::after");
    const [r, g, b] = cs.backgroundColor.match(/\d+/g)!.map(Number);
    return {
      display: cs.display,
      height: cs.height,
      bg: "#" + [r, g, b].map((n) => n.toString(16).padStart(2, "0")).join(""),
    };
  });
}

/** Landscape phone — which is the WIDE layout (over 700px), the one an unfolded
 *  Fold is always in: no destination bar. */
const LANDSCAPE = { width: PHONE.height, height: PHONE.width };

test("the chrome clears the notch, the cutout and the home indicator", async ({ page }) => {
  await boot(page);

  // The control: with no insets nothing is padded, so whatever moves below is the
  // insets moving it and not padding that was always there.
  expect(await pad(page, "header", "top"), "the header's own padding").toBe(10);
  expect(await pad(page, ".frame", "left")).toBe(0);
  expect(await pad(page, ".frame", "right")).toBe(0);
  expect(await pad(page, "nav.bottom-nav", "bottom")).toBe(0);
  const flush = (await page.locator(".pane").first().boundingBox())!;
  expect(flush.x).toBe(0);

  await notch(page, true);

  // Its own 10px plus the inset, not one restated total.
  expect(await pad(page, "header", "top"), "the header is under the status bar").toBe(
    10 + INSET.top,
  );

  // Landscape left/right sit on the frame, so the reader — which has no chrome of
  // its own to do it — is inset too.
  expect(await pad(page, ".frame", "left")).toBe(INSET.left);
  expect(await pad(page, ".frame", "right")).toBe(INSET.right);
  const inset = (await page.locator(".pane").first().boundingBox())!;
  expect(inset.x, "the reader runs under the camera cutout in landscape").toBeGreaterThanOrEqual(
    INSET.left,
  );
  expect(
    inset.x + inset.width,
    "the reader runs under the cutout on the other side",
  ).toBeLessThanOrEqual(PHONE.width - INSET.right + 0.5);

  // The one that already worked, pinned so it cannot regress.
  expect(await pad(page, "nav.bottom-nav", "bottom"), "the destination bar").toBe(INSET.bottom);

  // An inset that stuck would be a permanent margin on every device without a notch.
  await notch(page, false);
  expect(await pad(page, "header", "top")).toBe(10);
  expect((await page.locator(".pane").first().boundingBox())!.x).toBe(0);
});

// Present is `position: fixed`: it escapes the frame entirely and covers the status
// bar, so it has to carry all four insets itself.
test("Present clears the notch on all four sides", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => ((window as any).__plumbline.showPresent = true));
  await expect(page.locator(".present")).toBeVisible({ timeout: 20_000 });

  await notch(page, true);
  expect(await pad(page, ".present", "top"), "Present is under the status bar").toBe(INSET.top);
  expect(await pad(page, ".present", "left")).toBe(INSET.left);
  expect(await pad(page, ".present", "right")).toBe(INSET.right);

  // Portrait: the destination bar is on screen and already carries the inset inside
  // its measured height, so Present must stop at the bar and not one home indicator
  // further up — counting it twice leaves a dead band. Polled because `--bottomNavH`
  // is republished by a ResizeObserver after the bar changes height.
  await expect
    .poll(
      async () => {
        const bar = (await page.locator("nav.bottom-nav").boundingBox())!;
        const pres = (await page.locator(".present").boundingBox())!;
        return Math.round(pres.y + pres.height) - Math.round(bar.y);
      },
      { message: "Present double-counted the home indicator: it stops short of the bar" },
    )
    .toBe(0);
  expect(await pad(page, ".present", "bottom"), "the bar carries the inset in portrait").toBe(0);

  // Landscape: no destination bar, so Present runs to the edge and carries the inset
  // itself, as padding — the cream, not the frame behind it, is what sits under the
  // home indicator. (It used to stop AT the inset and leave the frame showing through
  // the gap: a dark band under a cream screen on a dark theme.) The bar's measured
  // height falling to zero is what makes this leg sensitive to the `max()`, so that
  // is asserted rather than assumed.
  await page.setViewportSize(LANDSCAPE);
  await expect(page.locator("nav.bottom-nav")).toBeHidden();
  await expect
    .poll(async () =>
      page.evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue("--bottomNavH").trim(),
      ),
    )
    .toBe("0px");
  const land = (await page.locator(".present").boundingBox())!;
  expect(Math.round(land.y + land.height), "Present runs to the bottom edge").toBe(PHONE.width);
  expect(await pad(page, ".present", "bottom"), "and pads by the inset").toBe(INSET.bottom);
});

// Where no bar carries the inset — the wide layout, every landscape phone and an
// unfolded Fold — the frame paints the bar's surface under the home indicator
// itself. Before this the colour there was whatever was LAST in the frame: the
// canon strip on the reader, each screen's paper on a destination. It changed
// with every screen and every fold and never agreed with the status bar
// (maintainer, 2026-09-21). Can fail: without the rule the pseudo-element has
// no `content`, its computed height is "auto" rather than the inset, and the
// body runs to the bottom edge.
test("the wide layout paints the destination bar's surface under the home indicator", async ({ page }) => {
  await page.setViewportSize(LANDSCAPE);
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator("nav.bottom-nav")).toBeHidden();

  // The control: no inset, nothing to see — at 0px the strip is nothing at all.
  expect((await foot(page)).height).toBe("0px");

  await notch(page, true);
  const strip = await foot(page);
  expect(strip.display, "the strip exists where the bar does not").not.toBe("none");
  expect(strip.height).toBe(`${INSET.bottom}px`);
  // The SAME surface the status bar is told about (session.chrome → paneNavBg),
  // read off the session rather than a literal so a palette change cannot strand it.
  expect(strip.bg).toBe(
    await page.evaluate(() => (window as any).__plumbline.palette.paneNavBg.toLowerCase()),
  );
  // And the body above it shrinks by as much: nothing of the app ends under the pill.
  const body = (await page.locator(".frame > .body").boundingBox())!;
  expect(Math.round(body.y + body.height)).toBeLessThanOrEqual(PHONE.width - INSET.bottom);

  // A destination's screen too — the strip is the frame's, not the reader's.
  await page.locator(".browse").getByRole("button", { name: "Study" }).click();
  await expect(page.locator(".frame")).toHaveAttribute("data-screen", "study");
  expect((await foot(page)).height).toBe(`${INSET.bottom}px`);

  // Portrait: the bar is the surface, so the strip stands down — otherwise the
  // inset would be counted twice.
  await page.setViewportSize(PHONE);
  await expect(page.locator("nav.bottom-nav")).toBeVisible();
  expect((await foot(page)).display).toBe("none");
});

// Sing is Present's twin: fixed, cream, over everything — and until now it stopped
// at the inset the same way.
test("Sing carries the inset where no bar does", async ({ page }) => {
  await page.setViewportSize(LANDSCAPE);
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await page.locator(".browse").getByRole("button", { name: "Sing" }).click();
  await page.locator(".content button.row").first().click();
  await page.locator("button.sing").click();
  await expect(page.locator(".sing-host")).toBeVisible();

  await notch(page, true);
  const sing = (await page.locator(".sing-host").boundingBox())!;
  expect(Math.round(sing.y + sing.height), "Sing runs to the bottom edge").toBe(PHONE.width);
  expect(await pad(page, ".sing-host", "bottom")).toBe(INSET.bottom);
  expect(await pad(page, ".sing-host", "top")).toBe(INSET.top);
});
