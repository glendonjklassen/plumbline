import { expect, test, type Page } from "@playwright/test";

// Every engine read goes through one promise map (`#waiting` in
// engine/worker-client.ts), and nothing settled it except a reply — so a worker that
// went away (an uncaught throw during boot, an OOM kill on a phone, a reply that would
// not structured-clone) left every pending promise pending for ever, with the splash on
// its last phase, no error and nothing to retry.
//
// Three things end it now: an `error` handler, a `messageerror` handler, and a boot
// watchdog for a worker that is alive but silent. All three reject every pending call —
// which App.svelte renders as its boot error, with a Retry — and mark the client dead so
// a later call fails fast instead of queueing into a corpse.
//
// Not covered: a worker death AFTER boot surfaces only as a rejection to the caller plus
// the `onFatal` hook, since there is no post-boot fatal UI yet. The hook is where it
// will attach.

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

/** Boot far enough that the session exists: this needs the engine client, not the
 *  reader, so the first-run chooser may still own the screen. */
async function bootedSession(page: Page): Promise<void> {
  await page.goto("/");
  await page.waitForFunction(() => !!(window as any).__plumbline?.rpc, null, { timeout: 90_000 });
}

test("an engine worker that cannot start shows an error, not an endless splash", async ({ page }) => {
  // Aborting the request for the worker's own script fails its construction, which
  // raises the same `error` event a crash does. Without the handler this page paints
  // "Fetching scripture data — 0%" and stays there.
  await page.route(/engine\.worker/, (route) => route.abort());
  await page.goto("/");
  // The reader's sentence, not the machine's: the raw text is built at the throw site
  // and stays in the <details>.
  await expect(page.locator(".splash .error")).toContainText(/engine stopped before Plumbline finished opening/, {
    timeout: 30_000,
  });
  await expect(page.locator(".splash details pre")).toContainText(/The study engine stopped unexpectedly/);
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();
  // An error screen, not an error beside a progress bar still claiming progress.
  await expect(page.locator(".splash .bar")).toHaveCount(0);
});

// One boot, three cases: the cold first visit is the expensive part, and none of the
// three needs its own.
test("a dead or silent engine worker settles every call; a slow one is left alone", async ({ page }) => {
  await bootedSession(page);

  // ── 1. it dies mid-boot ────────────────────────────────────────────────────
  // A worker that throws while evaluating: the shape of an uncaught error in the real one.
  const died = await page.evaluate(`(async () => {
    ${HARNESS}
    const rpc = new Rpc({ workerUrl: workerOf("throw new Error('engine worker crashed (test)')") });
    const boot = await settle(rpc.boot(), 10_000);
    // Anything asked afterwards must fail at once, not join a queue nobody reads.
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
  // The thread is alive, so no `error` event fires; only the watchdog can catch this.
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
  // The risk of a watchdog is killing a cold first visit on a phone, which legitimately
  // spends minutes downloading and opening the text while reporting progress. So the
  // budget is SILENCE, not elapsed time: this worker takes ~8x its silence budget to
  // boot while talking the whole way, and a plain elapsed-time timer fails here.
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
  // The result only means something if the boot really did outlast the silence budget
  // several times over.
  expect(slow.ms, "the slow worker finished too quickly to have tested anything").toBeGreaterThan(1_000);
});
