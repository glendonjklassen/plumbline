import { expect, test, type Page } from "@playwright/test";

// The suggested-weave bundle is the pack's first `optional` file: 194
// machine-proposed links that ship inside the APK but are a DOWNLOAD here, and
// one nobody gets unless they ask in Settings.
//
// The stage only means something if nothing fetches it on its own, so that is
// what most of this file pins. Requests are observed with `page.on("request")`
// rather than intercepted: interception via `page.route()` bypasses service
// workers (the trap recorded in network.spec.ts), and here it would also change
// the very behaviour under test. Passive observation sees worker requests too,
// which matters because every pack fetch is made by the engine worker.
//
// Mutation-tested 2026-08-02:
//   * `stage: "optional"` → `"study"` in build-web-pack.mjs makes "a first visit
//     never fetches" go red — stage 2 pulls it on every first visit. Note the
//     pack CHECKER refuses that build first, so proving it meant disabling both
//     guards; the two layers agree.
//   * dropping the `if (readFile(root, path)) continue` guard in home.ts makes
//     "the reader's own copy survives" go red.
//
// The UPDATE path is not tested here — a reload alone never reaches
// `reconcilePack`, which runs only on a pack-version change. That case needs
// the rewriting origin, so it lives in network.spec.ts ("a deploy does not push
// the optional bundle onto a device that declined it"), mutation-tested there.
//
// This file found a real bug on first run: `installSuggestedWeaves` wrote all
// 194 files and the marker, but `suggestedInstalled` was a boolean captured at
// buildHome, so Settings went on offering a download the reader had already
// finished until the next reload. It is a getter over mutable state now.

test.setTimeout(240_000);

/** The bundle's path in the manifest — matched loosely against request URLs,
 *  which carry a `?h=` content hash. */
const BUNDLE = "weaves/suggested.bundle.json";

/** Every pack request the page and its workers made, in order. */
function watchPack(page: Page): string[] {
  const seen: string[] = [];
  page.on("request", (r) => {
    const u = r.url();
    if (u.includes("/pack/")) seen.push(u);
  });
  return seen;
}

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

/** Wait until the worker has finished its background stages, so "nothing
 *  fetched the bundle" is a statement about a settled boot rather than a race
 *  the test won by being early. Stage 2 landing is what unlocks Strong's. */
async function settle(page: Page): Promise<void> {
  await page.waitForFunction(
    async () => {
      const w = (window as any).__plumbline;
      if (!w?.rpc) return false;
      const st = await w.rpc.suggestedState().catch(() => null);
      return st !== null;
    },
    undefined,
    { timeout: 90_000 },
  );
  await page.waitForTimeout(2_000); // let any further stage work start if it would
}

async function openSettings(page: Page): Promise<void> {
  await page.evaluate(() => ((window as any).__plumbline.showSettings = true));
  // The pack rows live behind the Advanced disclosure now.
  await page.locator('[data-surface="settings"] details.advanced > summary').click();
  await expect(page.getByText("Suggested weaves")).toBeVisible({ timeout: 30_000 });
}

test("a first visit never fetches the optional bundle", async ({ page }) => {
  const packed = watchPack(page);
  await boot(page);
  await settle(page);

  // Plenty else was fetched — this is a real boot, not a silent no-op.
  expect(packed.length).toBeGreaterThan(0);
  expect(packed.filter((u) => u.includes(BUNDLE))).toEqual([]);

  // And the engine agrees about where it stands.
  const st = await page.evaluate(() => (window as any).__plumbline.rpc.suggestedState());
  expect(st).toMatchObject({ available: true, installed: false });
  expect(st.gzBytes).toBeGreaterThan(0);
});

test("asking for it downloads it once, and the weaves arrive", async ({ page }) => {
  const packed = watchPack(page);
  await boot(page);
  await settle(page);
  await openSettings(page);

  const before = await page.evaluate(async () => {
    const w = (window as any).__plumbline;
    return (await w.rpc.call("suggestedWeaves"))?.suggested?.length ?? 0;
  });

  await page.getByRole("button", { name: "Download", exact: true }).click();
  // The row rewrites itself rather than leaving a button that does nothing.
  await expect(page.getByText(/Installed\./)).toBeVisible({ timeout: 90_000 });
  await expect(page.getByRole("button", { name: "Download", exact: true })).toHaveCount(0);

  // Exactly one request for it — the click, and nothing speculative around it.
  expect(packed.filter((u) => u.includes(BUNDLE)).length).toBe(1);

  // The engine re-read its library: the suggestions are actually there now.
  const after = await page.evaluate(async () => {
    const w = (window as any).__plumbline;
    return (await w.rpc.call("suggestedWeaves"))?.suggested?.length ?? 0;
  });
  expect(after).toBeGreaterThan(before);
  expect(after).toBeGreaterThan(100); // the shipped set is 194 files
});

test("a reload after installing neither forgets nor re-downloads", async ({ page }) => {
  await boot(page);
  await settle(page);
  await openSettings(page);
  await page.getByRole("button", { name: "Download", exact: true }).click();
  await expect(page.getByText(/Installed\./)).toBeVisible({ timeout: 90_000 });
  const installed = await page.evaluate(async () => {
    const w = (window as any).__plumbline;
    return (await w.rpc.call("suggestedWeaves"))?.suggested?.length ?? 0;
  });

  // Second visit: the bundle is a download the reader has already made, so it
  // must not be made again — this is the case a naive "fetch what the manifest
  // lists" reconcile gets wrong.
  const packed = watchPack(page);
  await page.reload();
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  await settle(page);

  expect(packed.filter((u) => u.includes(BUNDLE))).toEqual([]);
  const st = await page.evaluate(() => (window as any).__plumbline.rpc.suggestedState());
  expect(st.installed).toBe(true);
  const still = await page.evaluate(async () => {
    const w = (window as any).__plumbline;
    return (await w.rpc.call("suggestedWeaves"))?.suggested?.length ?? 0;
  });
  expect(still).toBe(installed);
});

/** One home file, straight into the user store — the same place `persistUserData`
 *  writes, so the next boot lays it into the home like any file of the reader's.
 *  There is no RPC for this on purpose: nothing in the product writes an
 *  arbitrary path, and a test-only endpoint would be a hole in the ABI. */
async function seedUserFile(page: Page, path: string, text: string): Promise<void> {
  await page.evaluate(
    async ({ path, text }) => {
      const db = await new Promise<IDBDatabase>((res, rej) => {
        const r = indexedDB.open("plumbline", 1);
        r.onsuccess = () => res(r.result);
        r.onerror = () => rej(r.error);
      });
      await new Promise<void>((res, rej) => {
        const tx = db.transaction("user", "readwrite");
        tx.objectStore("user").put(new TextEncoder().encode(text), path);
        tx.oncomplete = () => res();
        tx.onerror = () => rej(tx.error);
      });
    },
    { path, text },
  );
}

async function readUserFile(page: Page, path: string): Promise<string | null> {
  return page.evaluate(async (path) => {
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = indexedDB.open("plumbline", 1);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const raw = await new Promise<unknown>((res) => {
      const q = db.transaction("user").objectStore("user").get(path);
      q.onsuccess = () => res(q.result);
      q.onerror = () => res(null);
    });
    if (!raw) return null;
    const bytes = raw instanceof Uint8Array ? raw : new Uint8Array(raw as ArrayBuffer);
    return new TextDecoder().decode(bytes);
  }, path);
}

test("the reader's own copy of a suggestion survives the install", async ({ page }) => {
  await boot(page);
  await settle(page);

  // Put a file where one of the bundle's files would land, with bytes that are
  // unmistakably not the bundle's. THE READER'S COPY WINS is the rule the
  // seeded stock already follows, and an install is exactly when it is tested.
  // Seeded through IndexedDB and a reload, because the guard reads the LIVE
  // home and only a boot lays saved user files into it.
  const path = "weaves/suggested/accounted-as-sheep-for-the-slaughter.json";
  const mine = JSON.stringify({
    format: "overlay-weave-v2",
    name: "MINE — do not overwrite",
    kind: "quotation",
    tokenization: "kjv1769-tok2",
    notes: "",
    notesSource: "",
    created: "2026-01-01T00:00:00Z",
    approved: false,
    links: [],
  });
  await seedUserFile(page, path, mine);
  await page.reload();
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  await settle(page);

  await openSettings(page);
  await page.getByRole("button", { name: "Download", exact: true }).click();
  await expect(page.getByText(/Installed\./)).toBeVisible({ timeout: 90_000 });

  // Their bytes, untouched — and the rest of the set still arrived around it.
  expect(await readUserFile(page, path)).toContain("MINE — do not overwrite");
  const count = await page.evaluate(
    async () => ((await (window as any).__plumbline.rpc.call("suggestedWeaves"))?.suggested ?? []).length,
  );
  expect(count).toBeGreaterThan(100);
});
