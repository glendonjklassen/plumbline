import { expect, test, type Page } from "@playwright/test";

// The two smallest promises an installed PWA makes:
//
//   1. A launcher SHORTCUT is a URL (`?open=review`, manifest.webmanifest
//      `shortcuts`) stored by the OS at install time, answered on a cold start by
//      `launchDestination` (src/shell/church.ts). A destination is a screen, which
//      replaces the reader, so "it booted" and "it landed where the shortcut points"
//      are different claims. manifest.spec.ts holds the manifest's URLs to the
//      whitelist; this file holds the whitelist to the actual boot.
//   2. The icon BADGE is the due-card count (session.refreshAppBadge, the Badging
//      API). Nothing can push it from a server, so its truth is its three call
//      sites: boot (idle), resume, and every authoring write (`rpc.onAuthored`).
//
// Every shortcut test boots twice because a shortcut only exists on an installed
// app, which postdates the first run. The second entry leaves the origin so the
// `?open=` arrival is a real cold start, not a same-document navigation.

/** Light boot: the analysis tiers are irrelevant here, so the first-run checkboxes are
 *  left alone and no pack is downloaded for them. */
async function boot(page: Page, url = "/"): Promise<void> {
  await page.goto(url);
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

  // The empty-queue line is the review view's own copy (memorize.nothingDue), so
  // reaching it on a fresh profile proves the view and not just the screen. Exact,
  // because "Nothing due" is also the short form's whole text and the body's opening
  // words, and a substring match would trip strict mode.
  await expect(page.getByText("Nothing due.", { exact: true })).toBeVisible({ timeout: 90_000 });
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
  // "constellation" is a real surface but not a whitelisted destination: church.ts
  // treats an unrecognized query as a normal boot, never a blank screen.
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

  // Boot with no cards must clear, not skip: a reader who reviewed their last due
  // card yesterday still carries yesterday's badge.
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
