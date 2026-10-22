// Wait for the work a boot leaves running in the background to finish.
//
// After the first text is up, the engine keeps going: it warms the search and
// study indexes in slices, loads stage 2 and the analysis pack, and each stage
// that lands invalidates the session — every open surface re-fetches and
// repaints. A test that measures pixels or boxes in that window measures a
// surface that may be replaced under it, which is a timing failure and not the
// bug it is about. The faster the machine, the less often it happens, so it shows
// up as a flake under a loaded parallel run.
//
// A boot trace that has stopped growing says no further invalidation is coming
// (the technique a11y.spec.ts and reading.spec.ts use). `expect.poll` rather than
// `waitForFunction`: an async predicate there hands back a truthy promise on its
// first tick and waits for nothing.
//
// Only for a FIRST visit's tests. After a relaunch it waits out exactly the
// interval a relaunch measurement is about.

import { expect, type Page } from "@playwright/test";

export async function settleBackground(page: Page): Promise<void> {
  let seen = -1;
  await expect
    .poll(
      async () => {
        const n = await page.evaluate(
          async () => (((await (window as any).__plumbline.rpc.bootTrace()) ?? []) as unknown[]).length,
        );
        const settled = n === seen && n > 10;
        seen = n;
        return settled;
      },
      { timeout: 120_000, intervals: [1500] },
    )
    .toBe(true);
}
