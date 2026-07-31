import { expect, test, type Page } from "@playwright/test";

// AN EXCEPTION NOBODY CAUGHT USED TO LEAVE A SCREEN THAT SIMPLY STOPPED.
//
// There was no `error` or `unhandledrejection` handler anywhere in this product
// (audit D-12). A throw inside an effect, a component that failed to render, a
// promise nobody awaited — each one went to the console of a device that has no
// console, and the reader was left tapping a page that had quietly died. There
// was nothing to read and nothing to do.
//
// The bar is the whole feature, so these tests are about the three things that
// make it worth having rather than about its wording:
//   * it appears at all, for BOTH kinds of fault (they are separate events and
//     one handler does not catch the other);
//   * a STORM does not turn it into the failure — a render that throws throws
//     again on every reactive pass;
//   * it does not talk over the one error path that already reports itself well
//     (the splash's, D-11).
//
// NOT RUN by the agent that wrote this file — no Playwright in that sandbox. The
// mutation recipe for each test is on the test.

/** Boot to the reader. The bar is once per session, so every test here needs its
 *  own page — which Playwright gives it. */
async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Throw where nothing can catch it: a timer callback, which is how a real fault
 *  in an effect or a callback reaches `window.onerror`. Not `page.evaluate(() =>
 *  { throw … })`, whose throw belongs to the evaluate and never reaches the page
 *  at all. */
async function throwLoose(page: Page, message: string, times = 1): Promise<void> {
  await page.evaluate(
    ([m, n]) => {
      for (let i = 0; i < (n as number); i++) setTimeout(() => { throw new Error(`${m} #${i}`); }, 0);
    },
    [message, times] as const,
  );
}

const bar = (page: Page) => page.locator(".mishap");

// MUTATION: in App.svelte, delete the `addEventListener("error", …)`. Red: the
// bar never appears — which is the pre-D-12 app exactly.
test("an exception nobody caught raises a bar the reader can act on", async ({ page }) => {
  await boot(page);
  await throwLoose(page, "boom (test)");

  await expect(bar(page)).toBeVisible({ timeout: 15_000 });
  await expect(bar(page).locator(".what")).toHaveText("Something went wrong — reload");
  await expect(bar(page).getByRole("button", { name: "Reload" })).toBeVisible();
  // The raw string travels, for the same reason the splash's does: it is what a
  // bug report pastes.
  await expect(bar(page).locator("pre")).toContainText("boom (test)");
});

// MUTATION: in App.svelte, delete the `addEventListener("unhandledrejection", …)`.
// Red: the bar never appears. It is a SEPARATE event from `error` — the handler
// above catches none of this — which is why it has its own test.
test("a rejected promise nobody awaited raises it too", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    void Promise.reject(new Error("nope (test)"));
  });

  await expect(bar(page)).toBeVisible({ timeout: 15_000 });
  await expect(bar(page).locator("pre")).toContainText("nope (test)");
});

// MUTATION: in App.svelte, delete the `if (BENIGN.test(…)) return;` line. Red:
// the bar appears for a notice that means nothing is wrong.
//
// A SYNTHETIC event, because provoking a real ResizeObserver loop on demand is
// timing-dependent and would flake — but it goes through `window`'s own dispatch,
// so the app's listener handles it exactly as it would the browser's. The second
// half is the control: a handler that was simply removed would also "pass" the
// first half.
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

  // The control: the net is still armed for something that IS a fault.
  await throwLoose(page, "real fault (test)");
  await expect(bar(page)).toBeVisible({ timeout: 15_000 });
});

// MUTATION: in App.svelte's `noteMishap`, drop the `mishapSpent` guard (`if
// (leaving || !session) return;`) and the line that sets it. Red twice, and the
// second one is the one that matters: the detail becomes the LAST fault of the
// storm ("#24") because every event rewrote it, and then the bar the reader
// dismissed comes straight back on the next tick of the same broken loop.
//
// The COUNT assertion alone would NOT catch it — one `mishap` string can only
// ever render one bar — which is why the other two are here.
test("a storm of faults is one bar, and dismissing it is final", async ({ page }) => {
  await boot(page);
  await throwLoose(page, "storm (test)", 25);

  await expect(bar(page)).toBeVisible({ timeout: 15_000 });
  await expect(bar(page), "a storm of faults stacked up a wall of bars").toHaveCount(1);
  // The first fault is the one reported: re-writing the detail on every event
  // would repaint this bar 25 times while the reader is trying to read it.
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

// MUTATION: in App.svelte's `noteMishap`, drop `!session` from the guard. Red:
// the bar appears over the splash's own error screen, so the reader is told the
// same thing twice and the weaker telling is the one on top.
test("the splash's own boot error is not repeated by the bar", async ({ page }) => {
  // A stub engine worker that fails the boot RPC, so the splash owns the screen
  // with its error. (Boot's fetches happen inside the worker, where page-level
  // routing cannot see them — being the worker is the only way in.)
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
