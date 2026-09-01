import { expect, test, type Page } from "@playwright/test";

// With no `error` or `unhandledrejection` handler, an exception in an effect or a
// promise nobody awaited left the reader tapping a page that had quietly died, with
// nothing to read and nothing to retry. The failure bar is that net; these tests
// cover the three things that make it worth having — it appears for both kinds of
// fault, a storm of faults is still one bar, and it does not talk over the splash's
// own error screen.

/** Boot to the reader. The bar shows once per session, so each test needs its own page. */
async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Throw from a timer callback, which is how a real fault reaches `window.onerror`.
 *  A throw inside `page.evaluate` belongs to the evaluate and never reaches the page. */
async function throwLoose(page: Page, message: string, times = 1): Promise<void> {
  await page.evaluate(
    ([m, n]) => {
      for (let i = 0; i < (n as number); i++) setTimeout(() => { throw new Error(`${m} #${i}`); }, 0);
    },
    [message, times] as const,
  );
}

const bar = (page: Page) => page.locator(".mishap");

// Fails without App.svelte's `addEventListener("error", …)`: no bar ever appears.
test("an exception nobody caught raises a bar the reader can act on", async ({ page }) => {
  await boot(page);
  await throwLoose(page, "boom (test)");

  await expect(bar(page)).toBeVisible({ timeout: 15_000 });
  await expect(bar(page).locator(".what")).toHaveText("Something went wrong — reload");
  await expect(bar(page).getByRole("button", { name: "Reload" })).toBeVisible();
  // The raw string travels: it is what a bug report pastes.
  await expect(bar(page).locator("pre")).toContainText("boom (test)");
});

// Fails without App.svelte's `unhandledrejection` listener. It is a separate event
// from `error`, which the handler above does not catch — hence its own test.
test("a rejected promise nobody awaited raises it too", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    void Promise.reject(new Error("nope (test)"));
  });

  await expect(bar(page)).toBeVisible({ timeout: 15_000 });
  await expect(bar(page).locator("pre")).toContainText("nope (test)");
});

// Fails without App.svelte's `if (BENIGN.test(…)) return;`: the bar appears for a
// notice that means nothing is wrong. The event is synthetic because provoking a real
// ResizeObserver loop on demand would flake, but it goes through `window`'s own
// dispatch, so the app's listener handles it as it would the browser's.
test("the ResizeObserver loop notice is not a fault, and is not reported as one", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    dispatchEvent(
      new ErrorEvent("error", {
        message: "ResizeObserver loop completed with undelivered notifications.",
      }),
    );
  });
  await page.waitForTimeout(500);
  await expect(
    bar(page),
    "a window resize raised the failure bar — this notice means an observer ran out of passes, not that anything broke",
  ).toHaveCount(0);

  // The control: a removed handler would also pass the assertion above.
  await throwLoose(page, "real fault (test)");
  await expect(bar(page)).toBeVisible({ timeout: 15_000 });
});

// Fails without the `mishapSpent` guard in App.svelte's `noteMishap`: the detail
// becomes the storm's LAST fault because every event rewrites it, and a dismissed bar
// returns on the next tick. The count assertion alone cannot catch either — one
// `mishap` string only ever renders one bar — hence the other two.
test("a storm of faults is one bar, and dismissing it is final", async ({ page }) => {
  await boot(page);
  await throwLoose(page, "storm (test)", 25);

  await expect(bar(page)).toBeVisible({ timeout: 15_000 });
  await expect(bar(page), "a storm of faults stacked up a wall of bars").toHaveCount(1);
  // The first fault is the one reported: rewriting the detail on every event would
  // repaint the bar 25 times while the reader is trying to read it.
  await expect(bar(page).locator("pre")).toContainText("storm (test) #0");

  await bar(page).getByRole("button", { name: "Dismiss" }).click();
  await expect(bar(page)).toHaveCount(0);

  await throwLoose(page, "after dismissal (test)", 10);
  // Give it a real chance to come back before believing it did not.
  await page.waitForTimeout(1_000);
  await expect(
    bar(page),
    "the reader dismissed the bar and the next fault put it straight back — reloading is the only " +
      "remedy on offer, so asking again is only harassment",
  ).toHaveCount(0);
});

// Fails without `!session` in `noteMishap`'s guard: the bar covers the splash's own
// error screen, so the reader is told the same thing twice by the vaguer of the two.
test("the splash's own boot error is not repeated by the bar", async ({ page }) => {
  // A stub engine worker that fails the boot RPC, so the splash owns the screen with
  // its error. Boot's fetches happen inside the worker, where page-level routing
  // cannot see them, so replacing the worker is the only way in.
  await page.addInitScript(() => {
    const Real = window.Worker;
    const src =
      "self.onmessage = (e) => { if (e.data && e.data.op === 'boot') " +
      "self.postMessage({ id: e.data.id, error: 'engine open failed' }); };";
    class Stub extends Real {
      constructor(_url: string | URL, opts?: WorkerOptions) {
        super(URL.createObjectURL(new Blob([src], { type: "text/javascript" })), opts);
      }
    }
    (window as any).Worker = Stub;
  });
  await page.goto("/");
  await expect(page.locator(".splash .error")).toBeVisible({ timeout: 30_000 });

  await throwLoose(page, "during boot (test)", 3);
  await page.waitForTimeout(1_000);
  await expect(
    bar(page),
    "the boot error screen already says what happened and offers a Retry; the bar is a second, vaguer copy of it",
  ).toHaveCount(0);
  await expect(page.locator(".splash .error")).toBeVisible();
});
