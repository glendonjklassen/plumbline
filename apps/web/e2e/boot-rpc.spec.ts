import { expect, test, type Page } from "@playwright/test";

// THE ENGINE LIVES IN ONE THREAD, SO EVERY BOOT READ IS A QUEUE HOP.
//
// That is the rule the whole worker is built around (see the SCHEDULING RULE at
// the top of engine.worker.ts): one thread answers layout, taps and study reads,
// so anything the shell asks for before first text is not merely a round trip —
// it is a round trip that has to wait behind whatever the worker is already
// doing, on the one path where nothing else can proceed.
//
// Before audit F-11 the shell asked for SIX things to get to first text: the
// boot RPC, then three `themePalette` statics awaited together, then `toc` and
// `canonSegments` awaited together. Three sequential barriers for five reads
// that the boot reply could have carried, or that nothing before first text
// needed at all:
//
//   * the palettes are compiled-in colour tables (crates/core/src/theme.rs);
//   * the TOC is corpus-derived and cannot change while a session runs — which
//     is exactly why session.svelte.ts PINS it in the read-through cache;
//   * `canonSegments` is read by the canon strip, the passage navigator and the
//     maps. Nothing on the way to the text touches it, and all four read it
//     through `q()`, which fetches on first render and repaints when the answer
//     lands. Awaiting it made a read only the CHROME needs into a barrier in
//     front of the TEXT.
//
// Both halves are measured here, and the second one behaviourally: the counting
// test proves the reads are gone, and the starvation test proves the one that
// remains cannot hold the text up.
//
// The first-chapter display list, the third thing F-11 floats, is NOT folded in
// and is not tested here: a layout needs the pane's pixel width, which does not
// exist until the shell has mounted and measured itself, so the worker would
// have to guess it — and a display list laid out at the wrong width is a wasted
// layout that also poisons the turn cache.
//
// NOT RUN by the agent that wrote this file — no Playwright in that sandbox. The
// mutation recipe for each test is on the test.

/** Record the op of every message the page sends any worker, before any page
 *  script runs. `Worker.prototype.postMessage` is the one chokepoint the RPC
 *  client cannot avoid (backup.spec.ts patches the same seam). */
async function countOps(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const post = Worker.prototype.postMessage;
    (window as any).__ops = [];
    Worker.prototype.postMessage = function (this: Worker, ...args: any[]) {
      const m = args[0];
      if (m && typeof m === "object" && typeof m.op === "string") {
        (window as any).__ops.push(
          m.op === "call" ? `call:${m.method}` : m.op === "static" ? `static:${m.fn}` : m.op,
        );
      }
      return (post as any).apply(this, args);
    };
  });
}

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

// MUTATION, one per assertion:
//   a) App.svelte — put the three `rpc.static("themePalette", …)` calls back and
//      pass `{light, dark, night}` to initSession. Red: "3 themePalette round
//      trips" where the boot reply already had them.
//   b) App.svelte — `await s.fetchQ("toc")` still stands, so break the other
//      end: in worker-client.ts, drop `toc` from BOOT_READS. Red: "1 toc round
//      trip".
test("the boot reply carries the palettes and the TOC, so nothing asks for them", async ({ page }) => {
  await countOps(page);
  await boot(page);

  const ops: string[] = await page.evaluate(() => (window as any).__ops);
  const count = (op: string) => ops.filter((o) => o === op).length;

  // The control. Without it every assertion below is satisfied by an
  // interception that never saw a single message.
  expect(count("boot"), "the op counter never saw the boot message — the seam is not patched").toBe(1);
  expect(
    ops.some((o) => o === "layout"),
    "the op counter never saw a layout — this did not reach first text",
  ).toBe(true);

  expect(
    count("static:themePalette"),
    `${count("static:themePalette")} themePalette round trips before the reader could read anything — ` +
      "they are compiled-in colour tables and the boot reply carries them",
  ).toBe(0);
  expect(
    count("call:toc"),
    `${count("call:toc")} toc round trips — the boot reply carries the TOC, and it cannot change ` +
      "while a session runs",
  ).toBe(0);
});

// MUTATION: in App.svelte, put `await s.fetchQ("canonSegments")` back beside the
// TOC await. Red: the reader canvas never appears — the shell never mounts at
// all, because it is still waiting on a read the text does not need. (That is
// also the honest shape of the old bug: on a device where that read was slow
// rather than absent, the text waited exactly that long.)
test("a canon-segments read that never answers cannot hold up the text", async ({ page }) => {
  // Swallow it on the way out, so the worker never even hears the question and
  // the promise stays pending for the life of the page. A slow worker is the
  // real-world version of this; never-answers is the same thing with the timing
  // taken out.
  await page.addInitScript(() => {
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (this: Worker, ...args: any[]) {
      const m = args[0];
      if (m && m.op === "call" && m.method === "canonSegments") return;
      return (post as any).apply(this, args);
    };
  });

  await boot(page);
  // The text is there and readable, which is the whole assertion.
  // `.subtitle`, not the pane's own passage button: that button reads
  // "John\n      3 ▾" — a line break and a disclosure caret — so the obvious
  // /\w+ \d+/ never matches it. `.subtitle` is the header's plain "John 3" and
  // is what network.spec.ts already uses for exactly this assertion.
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 30_000 });
  // And the chrome that DOES need the canon degrades quietly rather than
  // breaking — the strip is still a named control, it just has nothing to paint.
  await expect(page.getByRole("slider", { name: "Jump to a book" })).toBeVisible();
});
