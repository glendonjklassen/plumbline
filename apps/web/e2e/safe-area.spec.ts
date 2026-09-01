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

  // Landscape: no destination bar, so the inset is the only thing holding Present off
  // the home indicator. Its measured height falling to zero is what makes this leg
  // sensitive to the `max()`, so that is asserted rather than assumed.
  await page.setViewportSize({ width: PHONE.height, height: PHONE.width });
  await expect(page.locator("nav.bottom-nav")).toBeHidden();
  await expect
    .poll(async () =>
      page.evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue("--bottomNavH").trim(),
      ),
    )
    .toBe("0px");
  const land = (await page.locator(".present").boundingBox())!;
  expect(
    Math.round(land.y + land.height),
    "Present runs under the home indicator",
  ).toBeLessThanOrEqual(PHONE.width - INSET.bottom);
});
