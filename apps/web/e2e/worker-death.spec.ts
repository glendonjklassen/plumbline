import { expect, test, type Page } from "@playwright/test";

// THE ENGINE WORKER DYING USED TO BE THE QUIETEST FAILURE IN THE APP.
//
// Every engine read goes through one promise map (`#waiting` in
// engine/worker-client.ts). Nothing settled that map except a reply, so if the
// worker went away — an uncaught throw during boot, an OOM kill on a phone, a
// reply that would not structured-clone — every pending promise simply stayed
// pending. The splash sat on its last phase, or the reader on a spinner, with no
// error, no explanation and nothing to retry. Forever.
//
// Three things now end it: an `error` handler, a `messageerror` handler, and a
// boot watchdog for the case where the worker is technically alive but has gone
// silent. All three reject every pending call, which is what the shell already
// turns into its boot error (App.svelte renders whatever `boot()` throws, with a
// Retry) — and mark the client dead so a later call fails fast instead of
// queueing into a corpse.
//
// WHAT IS NOT COVERED, honestly. A worker death AFTER boot is surfaced only as a
// rejection to whoever asked plus the `onFatal` hook; the shell has no
// post-boot fatal UI to show yet (the splash is gone by then), so a reader who
// loses the worker mid-session sees reads stop answering rather than a notice.
// The hook is where that UI will attach.

/** The blob-worker plumbing shared by the cases below, evaluated in the page. */
const HARNESS = `
  // The class under test, reached through the shell's own instance — nothing
  // exports it to the page, and in a production build the module has a hashed
  // name.
  const Rpc = Object.getPrototypeOf(window.__plumbline.rpc).constructor;
  const workerOf = (src) =>
    URL.createObjectURL(new Blob([src], { type: "text/javascript" }));
  // Never await a promise this test expects to hang: report the hang instead,
  // so a regression fails with a message rather than with a suite timeout.
  const settle = (p, ms) =>
    Promise.race([
      p.then(() => "resolved", (e) => "rejected: " + (e && e.message ? e.message : e)),
      new Promise((r) => setTimeout(() => r("HUNG — the promise never settled"), ms)),
    ]);
`;

/** Boot far enough that the session exists (the first-run chooser may still own
 *  the screen — this needs the engine client, not the reader). */
async function bootedSession(page: Page): Promise<void> {
  await page.goto("/");
  await page.waitForFunction(() => !!(window as any).__plumbline?.rpc, null, { timeout: 90_000 });
}

test("an engine worker that cannot start shows an error, not an endless splash", async ({ page }) => {
  // The real app, the real splash, and a real worker that dies: abort the
  // request for the worker's own script and its construction fails, which is the
  // same `error` event a crash raises. Before the handler existed this page
  // painted "Fetching scripture data — 0%" and stayed there.
  await page.route(/engine\.worker/, (route) => route.abort());
  await page.goto("/");
  // The reader's sentence, not the machine's: the raw "The study engine stopped
  // unexpectedly — …" is built at the throw site and stays in the <details>.
  await expect(page.locator(".splash .error")).toContainText(/engine stopped before Plumbline finished opening/, {
    timeout: 30_000,
  });
  await expect(page.locator(".splash details pre")).toContainText(/The study engine stopped unexpectedly/);
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();
  // And it is an ERROR screen, not an error next to a progress bar that is
  // still pretending something is happening.
  await expect(page.locator(".splash .bar")).toHaveCount(0);
});

// One boot, three cases: a cold first visit is the expensive part of this file,
// and none of these three needs its own.
test("a dead or silent engine worker settles every call; a slow one is left alone", async ({ page }) => {
  await bootedSession(page);

  // ── 1. it dies mid-boot ────────────────────────────────────────────────────
  // A module worker that throws while evaluating: the same shape as an uncaught
  // error inside the real one.
  const died = await page.evaluate(`(async () => {
    ${HARNESS}
    const rpc = new Rpc({ workerUrl: workerOf("throw new Error('engine worker crashed (test)')") });
    const boot = await settle(rpc.boot(), 10_000);
    // Anything asked AFTERWARDS must fail immediately rather than join a queue
    // nothing will ever read.
    const t0 = performance.now();
    const later = await settle(rpc.call("toc"), 10_000);
    return { boot, later, laterMs: performance.now() - t0 };
  })()`) as { boot: string; later: string; laterMs: number };

  expect(
    died.boot,
    "the engine worker died during boot and boot() never settled — this is the endless splash",
  ).toContain("The study engine stopped unexpectedly");
  expect(
    died.later,
    "a call made after the worker died never settled — it queued into a corpse",
  ).toContain("The study engine stopped unexpectedly");
  expect(died.laterMs, "a call to a dead worker should fail at once, not wait").toBeLessThan(1_000);

  // ── 2. it lives, but never answers ────────────────────────────────────────
  // Nothing above catches this: the thread is alive, so no `error` event ever
  // fires. The watchdog is the only thing that can.
  const silent = await page.evaluate(`(async () => {
    ${HARNESS}
    const rpc = new Rpc({
      workerUrl: workerOf("self.onmessage = () => {};"),
      bootSilenceMs: 500,
    });
    return await settle(rpc.boot(), 10_000);
  })()`) as string;
  expect(
    silent,
    "the engine worker accepted the boot message and never answered; boot() never settled",
  ).toContain("went quiet");

  // ── 3. slow is not dead ───────────────────────────────────────────────────
  // The whole risk of a watchdog: killing a cold first visit on a phone, which
  // legitimately spends minutes downloading and opening the text. It reports
  // progress throughout, so the budget is SILENCE, not elapsed time. This worker
  // takes ~8x its silence budget to boot while talking the whole way, and must
  // be allowed to finish — a plain elapsed-time timer fails here.
  const slow = await page.evaluate(`(async () => {
    ${HARNESS}
    const rpc = new Rpc({
      workerUrl: workerOf(\`
        self.onmessage = (ev) => {
          const m = ev.data;
          if (m.op !== "boot") return;            // the visibility ping
          let n = 0;
          const t = setInterval(() => {
            if (++n <= 8) {
              self.postMessage({ type: "progress", phase: "download", fraction: n / 10 });
              return;
            }
            clearInterval(t);
            self.postMessage({ id: m.id, result: { packVersion: "test", version: "test" } });
          }, 120);
        };
      \`),
      bootSilenceMs: 500,
    });
    const t0 = performance.now();
    const boot = await settle(rpc.boot(), 15_000);
    return { boot, ms: performance.now() - t0 };
  })()`) as { boot: string; ms: number };
  expect(
    slow.boot,
    "a worker that was reporting progress the whole time was declared dead — the watchdog is " +
      "measuring elapsed time instead of silence, and it will kill cold boots on phones",
  ).toBe("resolved");
  // The measurement only means something if this boot really did outlast the
  // budget several times over.
  expect(slow.ms, "the slow worker finished too quickly to have tested anything").toBeGreaterThan(1_000);
});
