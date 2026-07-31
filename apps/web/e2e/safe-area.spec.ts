import { expect, test, type Page } from "@playwright/test";

// The notch, held to account.
//
// `index.html` asks for `viewport-fit=cover`, so an installed PWA gets the whole
// screen — including the strip behind the status bar, the strip behind the home
// indicator, and (held sideways, which is how someone reads) the column behind
// the camera cutout. Only the bottom bar ever inset itself: the header sat under
// the clock, Present's ✕ with it, and in landscape the reader's first characters
// were behind the notch.
//
// THIS IS A BEHAVIOUR TEST OF A THING A BROWSER CANNOT SIMULATE. There is no
// Playwright, CDP or emulation switch that gives a page a safe-area inset, so a
// fix written directly in `env(safe-area-inset-*)` can only ever be read, never
// run — and unreadable-therefore-unrun is how the bottom bar came to be the only
// surface that had one. `app.css` names the four insets once as custom
// properties whose values ARE those `env()`s, so the app is unchanged on a
// square screen and this file can set them and watch the chrome move.
//
// What that costs in fidelity is worth stating: this proves every surface
// consumes the variables, not that the variables carry the OS's numbers. The
// second is one `env()` per side in one rule, checked by reading it.

const PHONE = { width: 390, height: 844 };

/** Distinct per side, so a rule wired to the wrong inset fails instead of passing. */
const INSET = { top: 44, right: 48, bottom: 34, left: 47 };

async function boot(page: Page): Promise<void> {
  await page.setViewportSize(PHONE);
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
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

// Mutation: reverting `padding-top` on `header` to plain `var(--headerPadY)` →
//   'Error: the header is under the status bar  expect(received).toBe(expected)
//    Expected: 54  Received: 10'.
// Mutation: dropping `padding-left`/`padding-right` from `.frame` → 'Error: the
//   reader runs under the camera cutout in landscape
//   expect(received).toBeGreaterThanOrEqual(expected)  Expected: >= 47
//   Received: 0'.
test("the chrome clears the notch, the cutout and the home indicator", async ({ page }) => {
  await boot(page);

  // The control: with no insets nothing is padded, so whatever moves below is
  // the insets moving it and not some other padding that was always there.
  expect(await pad(page, "header", "top"), "the header's own padding").toBe(10);
  expect(await pad(page, ".frame", "left")).toBe(0);
  expect(await pad(page, ".frame", "right")).toBe(0);
  expect(await pad(page, "nav.bottom-nav", "bottom")).toBe(0);
  const flush = (await page.locator(".pane").first().boundingBox())!;
  expect(flush.x).toBe(0);

  await notch(page, true);

  // The header, under the status bar until now: its own 10px PLUS the inset, not
  // one restated total.
  expect(await pad(page, "header", "top"), "the header is under the status bar").toBe(
    10 + INSET.top,
  );

  // Landscape left/right. The frame carries these so the READER is inset too —
  // it is the surface with no chrome of its own to do it.
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

  // The one that already worked, so it cannot regress while the others are added.
  expect(await pad(page, "nav.bottom-nav", "bottom"), "the destination bar").toBe(INSET.bottom);

  // And it all goes away again — an inset that sticks would be a permanent
  // margin on every device without a notch.
  await notch(page, false);
  expect(await pad(page, "header", "top")).toBe(10);
  expect((await page.locator(".pane").first().boundingBox())!.x).toBe(0);
});

// Present is `position: fixed`: it escapes the frame entirely, covers the status
// bar, and is the screen most likely to be held up sideways in front of someone.
//
// Mutation: dropping the `padding` line from `.present` → 'Error: Present is
//   under the status bar  expect(received).toBe(expected)  Expected: 44
//   Received: 0'.
// Mutation: `bottom: var(--bottomNavH, 0px)` (i.e. no `max`) with the bar hidden
//   → 'Error: Present runs under the home indicator  expect(received)
//    .toBeLessThanOrEqual(expected)  Expected: <= 810  Received: 844'.
test("Present clears the notch on all four sides", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => ((window as any).__plumbline.showPresent = true));
  await expect(page.locator(".present")).toBeVisible({ timeout: 20_000 });

  await notch(page, true);
  expect(await pad(page, ".present", "top"), "Present is under the status bar").toBe(INSET.top);
  expect(await pad(page, ".present", "left")).toBe(INSET.left);
  expect(await pad(page, ".present", "right")).toBe(INSET.right);

  // Portrait: the destination bar is on screen and already carries the inset
  // inside its own measured height, so Present stops at the bar and NOT one home
  // indicator further up. Counting it twice would leave a dead band.
  //
  // Polled, because the bar just got taller and `--bottomNavH` is republished by
  // a ResizeObserver — reading the two boxes once could catch the frame before
  // Present has been told.
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

  // Landscape: no destination bar at all (it is a phone affordance and the
  // viewport is now wide), so the inset is the only thing holding Present off the
  // home indicator. The bar's measured height really does fall to zero when it
  // goes — which is what makes this leg sensitive to the `max()` at all, so it is
  // asserted rather than assumed.
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
