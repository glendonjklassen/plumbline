import { expect, test, type Page } from "@playwright/test";

// The two smallest promises an installed PWA makes, and how each one breaks:
//
//   1. A launcher SHORTCUT is a URL (`?open=review`, manifest.webmanifest
//      `shortcuts`) stored by the OS at install time. The app answers it on a
//      COLD START in App.svelte via `launchDestination` (src/shell/church.ts) —
//      and a destination is a screen, which REPLACES the reader (Shell.svelte),
//      so "it booted" and "it landed where the shortcut points" are different
//      claims. manifest.spec.ts holds the manifest's URLs to the whitelist;
//      this file holds the whitelist to the actual boot.
//
//   2. The icon BADGE is the due-card count (session.refreshAppBadge, the
//      Badging API). It can only move while the app runs — there is no server
//      to push from — so its truth is the three call sites: boot (idle),
//      resume, and every authoring write (`rpc.onAuthored`). The test below
//      exercises the authoring one end to end: a card added through the real
//      engine must land on the (stubbed) OS API without any shell code being
//      poked directly.
//
// WHY EVERY SHORTCUT TEST BOOTS TWICE. A shortcut exists only on an INSTALLED
// app, and an install postdates the first run — so the profile that taps one
// has always finished the welcome. The plain boot() first run mirrors that;
// the second entry leaves the origin (about:blank) so the `?open=` arrival is
// a real cold start, not a same-document navigation (routing.spec.ts's note).
//
// MUTATION AUDIT (2026-08-08, all three run against the built bundle, each
// restored before the next):
//   1. App.svelte — delete the `if (opened === "hymnal") … else if (opened)`
//      block. → "a Review-due shortcut cold-starts into the drill" and the
//      hymnal test both red (screen stays "read").
//   2. shell/church.ts — `launchDestination` returns null for "review". →
//      manifest.spec.ts "every launcher shortcut names a destination the app
//      itself routes" red, AND the review test here red — the two failures
//      that together say "whitelist and manifest agree, and both are wrong".
//   3. state/session.svelte.ts — delete `this.refreshAppBadge()` from
//      `rpc.onAuthored`. → "the installed icon's badge is the due-card count"
//      red at the first poll (the boot-idle call still fires, so the recorded
//      tail stays "clear" — proof the test isolates the authoring site).

/** routing.spec.ts's light boot: the analysis tiers are irrelevant here, so the
 *  first-run checkboxes are left alone and no pack is downloaded for them. */
async function boot(page: Page, url = "/"): Promise<void> {
  await page.goto(url);
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Re-enter at `url` from outside the origin: a real cold start, as the
 *  launcher's stored shortcut URL would arrive. */
async function reenter(page: Page, url: string): Promise<void> {
  await page.goto("about:blank");
  await page.goto(url);
}

const shellState = (page: Page): Promise<{ screen: string; view: string | null; search: string }> =>
  page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { screen: s?.screen ?? "", view: s?.memorize?.view ?? null, search: location.search };
  });

test("a Review-due shortcut cold-starts into the drill, and consumes its query", async ({ page }) => {
  await boot(page);
  await reenter(page, "/?open=review");

  // The empty-queue line is the review view's own copy (memorize.nothingDue) —
  // a fresh profile has no cards, so reaching it proves the VIEW, not just the
  // screen. The state check underneath pins where the app thinks it is.
  await expect(page.getByText("Nothing due — well kept.")).toBeVisible({ timeout: 90_000 });
  expect(await shellState(page)).toEqual({ screen: "memorize", view: "review", search: "" });
});

test("a Hymnal shortcut cold-starts into the hymnal", async ({ page }) => {
  await boot(page);
  await reenter(page, "/?open=hymnal");

  await expect(page.locator('section[aria-label="Hymnal"]')).toBeVisible({ timeout: 90_000 });
  expect((await shellState(page)).search, "the shortcut query must not survive into the address bar").toBe("");
});

test("an ?open value the app does not route falls through to the reader", async ({ page }) => {
  await boot(page);
  // "constellation" is a real surface but NOT a whitelisted destination — the
  // adversarial neighbour of a valid value, per church.ts's stance that an
  // unrecognized query is a normal boot, never a blank screen.
  await reenter(page, "/?open=constellation");

  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  expect((await shellState(page)).screen).toBe("read");
});

test("the installed icon's badge is the due-card count", async ({ page }) => {
  // The stub must be in place before any app script runs: the boot-idle call
  // fires early, and a feature-detect that ran first would cache the miss.
  await page.addInitScript(() => {
    const calls: (number | "clear")[] = [];
    (window as any).__badgeCalls = calls;
    Object.defineProperty(navigator, "setAppBadge", {
      configurable: true,
      value: (n: number) => {
        calls.push(n);
        return Promise.resolve();
      },
    });
    Object.defineProperty(navigator, "clearAppBadge", {
      configurable: true,
      value: () => {
        calls.push("clear");
        return Promise.resolve();
      },
    });
  });
  await boot(page);

  // Boot, no cards: the idle refresh must CLEAR, not skip — a reader who
  // reviewed their last due card yesterday still carries yesterday's badge.
  await expect
    .poll(() => page.evaluate(() => (window as any).__badgeCalls.at(-1) ?? null), {
      message: "boot never touched the badge — the idle call site is gone",
      timeout: 30_000,
    })
    .toBe("clear");

  // A card seeded through the real engine is due immediately; the worker's
  // `authored` event is the only thing connecting that write to the badge.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.memoryAdd("John 3:16", new Date().toISOString());
  });
  await expect
    .poll(() => page.evaluate(() => (window as any).__badgeCalls.at(-1) ?? null), {
      message: "a due card never reached the badge — the authored call site is gone",
      timeout: 30_000,
    })
    .toBe(1);

  // And the write that empties the queue takes the badge with it.
  await page.evaluate(async () => {
    await (window as any).__plumbline.engine.memoryRemove("John 3:16");
  });
  await expect
    .poll(() => page.evaluate(() => (window as any).__badgeCalls.at(-1) ?? null), {
      message: "removing the last due card left the badge standing",
      timeout: 30_000,
    })
    .toBe("clear");
});
