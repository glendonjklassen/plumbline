import { expect, test, type Page, type Worker } from "@playwright/test";

// SAVING THE READER'S OWN WORK, AND SAYING SO WHEN IT DOES NOT HAPPEN.
//
// The authored subtree is mirrored to IndexedDB by the engine worker. Two ways
// that used to fail silently, both of them ending with the reader believing a
// note was saved when it existed only in a tab that was about to close:
//
//  1. `void booted.home.persistUserData()`. A QuotaExceededError — a phone with a
//     full disk, Safari's tighter budget, an origin whose database the browser has
//     decided to refuse — rejected a promise nobody held. Nothing was told: not
//     the shell, not the reader, who had already watched the note sheet close.
//  2. A 50 ms debounce. Write a note, switch apps: a hidden page has its timers
//     frozen and can be discarded outright, so the pending callback simply never
//     runs and the note goes with the tab.
//
// The fixes are a `persistFailed` message → a sticky notice with a retry, a
// backoff ladder in the worker, and an `{op:"flush"}` RPC the session calls on
// pagehide / visibilitychange-hidden.
//
// HOW THESE TESTS REACH THE FAILURE. The write happens in the ENGINE WORKER, so
// patching `indexedDB` from the page would patch the wrong thread — and
// `page.route()` would be worse than useless here (see e2e/network.spec.ts for
// what that already cost once). Playwright can evaluate inside a dedicated
// worker, so the failure is injected where the write really is: the worker's own
// `IDBObjectStore.prototype.put` throws a genuine QuotaExceededError.
//
// Mutation-tested 2026-07-29 — see the note above each case for what was broken
// and the exact assertion that went red.

test.setTimeout(240_000); // two engine boots

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** The engine worker — the thread that owns the home and the IndexedDB writes.
 *  Named `engine.worker` in dev and `engine.worker-<hash>.js` in the build. */
async function engineWorker(page: Page): Promise<Worker> {
  const found = page.workers().find((w) => /engine\.worker/.test(w.url()));
  return found ?? (await page.waitForEvent("worker", { predicate: (w) => /engine\.worker/.test(w.url()) }));
}

/** Make the worker's writes to the user store fail with a real
 *  QuotaExceededError. `budget` is how many attempts to refuse: -1 for every one
 *  of them, 0 to stop refusing (the repair). Counts attempts on the way through,
 *  which is how the retry is observed — at the IndexedDB boundary itself, not
 *  through anything the app chose to report. */
function setQuotaFailure(w: Worker, budget: number): Promise<void> {
  return w.evaluate((n) => {
    const g = self as any;
    g.__putBudget = n;
    if (g.__putPatched) return;
    g.__putPatched = true;
    g.__putAttempts = 0;
    const real = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (this: IDBObjectStore, ...args: any[]) {
      if (this.name === "user") {
        g.__putAttempts++;
        if (g.__putBudget !== 0) {
          if (g.__putBudget > 0) g.__putBudget--;
          // A DOMException with the name a browser actually uses. Thrown from
          // `put` rather than delivered as a transaction abort: both shapes exist
          // in the wild (Safari throws), and the one the shell must survive is
          // whichever the device chooses.
          throw new DOMException("Quota exceeded (test).", "QuotaExceededError");
        }
      }
      return (real as any).apply(this, args);
    };
  }, budget);
}

const putAttempts = (w: Worker): Promise<number> => w.evaluate(() => (self as any).__putAttempts ?? 0);

/** How many user-store VALUES contain `needle`. By value, not by key: an edit
 *  reuses its key, so key polling would pass before the write it waits for. */
function userValuesContaining(page: Page, needle: string): Promise<number> {
  return page.evaluate(async (n) => {
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = indexedDB.open("plumbline", 1);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const values = await new Promise<unknown[]>((res) => {
      const q = db.transaction("user").objectStore("user").getAll();
      q.onsuccess = () => res(q.result);
      q.onerror = () => res([]);
    });
    db.close();
    const dec = new TextDecoder();
    return values.filter((v) => dec.decode(v as Uint8Array).includes(n)).length;
  }, needle);
}

// Broken for mutation evidence by restoring the old fire-and-forget line in
// engine.worker.ts (`void booted!.home.persistUserData()` in place of
// `schedulePersist()`), and rebuilt. Red on the first assertion:
//   Error: the save to IndexedDB failed and the reader was never told — the note
//   exists only in this tab
//   Timed out 20000ms waiting for expect(locator).toBeVisible()
//   Locator: locator('.toast.warn')
test("a save this device refuses is reported, retried, and lands once it can", async ({ page }) => {
  await boot(page);
  const worker = await engineWorker(page);
  // Prove the pipeline works before breaking it, and let the first-run writes
  // settle, so the attempt counter below counts only what this test caused.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("John 3:16", "a note on a healthy disk", "2026-07-29T00:00:00Z");
  });
  await expect
    .poll(() => userValuesContaining(page, "a note on a healthy disk"), {
      message: "nothing was persisting even before the storage was broken",
      timeout: 20_000,
    })
    .toBeGreaterThan(0);
  await setQuotaFailure(worker, -1);

  // The reader writes a note. The engine takes it (into the in-memory home), so
  // as far as every UI affordance is concerned this succeeded.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("Gen 1:1", "a note on a full disk", "2026-07-29T00:00:00Z");
  });

  // 1. THE READER IS TOLD.
  const notice = page.locator(".toast.warn");
  await expect(
    notice,
    "the save to IndexedDB failed and the reader was never told — the note exists only in this tab",
  ).toBeVisible({ timeout: 20_000 });
  await expect(notice).toContainText(/Couldn't save your last change/);
  // And it is NOT lying: the note really is absent from storage.
  expect(
    await userValuesContaining(page, "a note on a full disk"),
    "the notice claimed a failure but the bytes did land — this test is not testing a failure",
  ).toBe(0);
  // Sticky, unlike every other toast in the app (2.2 s). A warning about the
  // reader's data that fades while they are looking at the note they just typed
  // is the same silence in a slower form.
  await page.waitForTimeout(3_500);
  await expect(notice, "the failure notice faded like an ordinary toast").toBeVisible();

  // 2. IT IS RETRIED, without the reader doing anything.
  await expect
    .poll(() => putAttempts(worker), {
      message: "the failed save was never retried — one refused write and the note was abandoned",
      timeout: 20_000,
    })
    .toBeGreaterThan(1);

  // 3. AND THE BACKLOG LANDS when the device can take it — no second authoring
  // write, no reload: the retry ladder carries what the first attempt dropped.
  await setQuotaFailure(worker, 0);
  await expect
    .poll(() => userValuesContaining(page, "a note on a full disk"), {
      message: "storage started accepting writes again and the abandoned note never followed",
      timeout: 30_000,
    })
    .toBeGreaterThan(0);
  await expect(notice, "the save succeeded but the notice stayed up").toBeHidden({ timeout: 10_000 });
});

// Broken for mutation evidence by removing the `retryPersist()` call from
// `flushSession()` in state/session.svelte.ts (leaving the config flush), and
// rebuilt. Red on the last assertion:
//   Error: the tab was put away 5 ms after the note was written and the note
//   never reached storage — the 50 ms debounce ate it
//   Timed out 20000ms waiting for expect.poll
//   Expected: > 0   Received: 0
test("a note written 5 ms before the tab is put away still reaches storage", async ({ page }) => {
  await boot(page);
  const worker = await engineWorker(page);

  // FREEZE THE WORKER'S SHORT TIMERS — which is not a contrivance, it is the
  // production failure. Chrome freezes a hidden page's timers and may discard the
  // tab outright, so the 50 ms debounce callback is exactly the one that never
  // runs. Only the 1–200 ms band goes: `yieldTask`'s setTimeout(…, 0) has to keep
  // working or the worker stops answering anything at all.
  await worker.evaluate(() => {
    const g = self as any;
    const real = self.setTimeout;
    g.__frozen = 0;
    (self as any).setTimeout = function (fn: any, ms?: number, ...rest: any[]) {
      if (typeof ms === "number" && ms > 0 && ms <= 200) {
        g.__frozen++;
        // A live handle that will never fire, so `clearTimeout` still has
        // something real to cancel.
        return real.call(self, () => {}, 1e9);
      }
      return real.call(self, fn, ms as any, ...rest);
    };
  });

  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("Ps 23:1", "written just before leaving", "2026-07-29T00:00:00Z");
    await new Promise((r) => setTimeout(r, 5));
  });

  // The debounce really is dead — without this the rest of the test would pass
  // against the very bug it describes.
  expect(
    await worker.evaluate(() => (self as any).__frozen ?? 0),
    "no short timer was suppressed, so the debounce could still have done the saving",
  ).toBeGreaterThan(0);
  expect(
    await userValuesContaining(page, "written just before leaving"),
    "the note reached storage without a flush — the debounce was not actually frozen",
  ).toBe(0);
  await page.waitForTimeout(800); // several debounce windows: still nothing
  expect(await userValuesContaining(page, "written just before leaving")).toBe(0);

  // Now the tab is put away. `pagehide` is what the browser fires; the session's
  // handler must turn it into an awaited flush rather than hoping the debounce
  // gets a chance. (visibilitychange-hidden runs the same handler, and is the one
  // that reliably completes on a phone.)
  await page.evaluate(() => dispatchEvent(new Event("pagehide")));

  await expect
    .poll(() => userValuesContaining(page, "written just before leaving"), {
      message:
        "the tab was put away 5 ms after the note was written and the note never reached storage — " +
        "the 50 ms debounce ate it",
      timeout: 20_000,
    })
    .toBeGreaterThan(0);
});
