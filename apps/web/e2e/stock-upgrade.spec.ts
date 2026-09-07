import { expect, test, type Page } from "@playwright/test";

// A stock file added in a later release must reach EXISTING installs, not only
// fresh ones. Seeding used to be all-or-nothing behind one boolean
// (`meta:stockSeeded`): once set, no boot ever seeded again, so a release that
// added a stock thread shipped it to nobody who already had the app — the
// maintainer's own install opened a shared link to the new thread and landed on
// the reader (2026-09-06, "just loads john 1...?").
//
// The fix in engine/home.ts: seeding is per file, recorded as the path set
// `meta:stockSeededPaths`. A path not in the set seeds; a path in it never
// seeds again, so edits and deletions still stick. An install carrying only the
// legacy boolean migrates as "the legacy stock set is seeded" — new paths seed,
// a deleted legacy path stays deleted.
//
// Why this can fail against the bug it describes (reasoned, not
// mutation-tested, per the repo's testing rules): the second boot below runs on
// a manufactured legacy store — `meta:stockSeeded` present, per-path record
// absent, the new thread's file absent. Under the old code that boot seeds
// nothing (the boolean is set), the file stays missing, and the "came back"
// assertion goes red. Under the per-path code the file is exactly what seeds.
// The third boot guards the other edge: with the file's path recorded as
// seeded, deleting it must be final — a naive "seed whatever is missing" would
// resurrect it and go red there.

test.setTimeout(360_000); // three engine boots share this test

/** The stock file this release added — the newest member of the stock set. */
const NEW_STOCK = "threads/how-to-be-saved.json";
const LEGACY_STOCK = "threads/romans-road.json";
const SEEDED_PATHS = "meta:stockSeededPaths";
const SEEDED_LEGACY = "meta:stockSeeded";

async function boot(page: Page): Promise<void> {
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** The keys present in one of the two stores. */
function storeKeys(page: Page, store: "user" | "cache"): Promise<string[]> {
  return page.evaluate(async (store) => {
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = indexedDB.open("plumbline", 1);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const keys = await new Promise<IDBValidKey[]>((res) => {
      const q = db.transaction(store).objectStore(store).getAllKeys();
      q.onsuccess = () => res(q.result);
      q.onerror = () => res([]);
    });
    db.close();
    return keys.map(String);
  }, store);
}

/** Rewrite the stores into the shape under test: delete `drop` keys, write
 *  `put` values (text, encoded the way engine/idb.ts stores bytes). */
function mutate(
  page: Page,
  edits: { user?: { drop?: string[] }; cache?: { drop?: string[]; put?: Record<string, string> } },
): Promise<void> {
  return page.evaluate(async (edits) => {
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = indexedDB.open("plumbline", 1);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const apply = (store: string, drop: string[], put: Record<string, string>) =>
      new Promise<void>((res, rej) => {
        const tx = db.transaction(store, "readwrite");
        const os = tx.objectStore(store);
        for (const k of drop) os.delete(k);
        for (const [k, v] of Object.entries(put)) os.put(new TextEncoder().encode(v), k);
        tx.oncomplete = () => res();
        tx.onerror = () => rej(tx.error);
      });
    await apply("user", edits.user?.drop ?? [], {});
    await apply("cache", edits.cache?.drop ?? [], edits.cache?.put ?? {});
    db.close();
  }, edits);
}

test("a stock file added in a release seeds into an existing install, once", async ({ page }) => {
  // Fresh install: everything seeds, and the per-path record says so.
  await page.goto("/");
  await boot(page);
  expect(await storeKeys(page, "user")).toContain(NEW_STOCK);
  expect(await storeKeys(page, "cache")).toContain(SEEDED_PATHS);

  // Manufacture the upgraded LEGACY install: the boolean marker of the
  // all-or-nothing era, no per-path record, and the new file never seeded.
  await mutate(page, {
    user: { drop: [NEW_STOCK] },
    cache: { drop: [SEEDED_PATHS], put: { [SEEDED_LEGACY]: "1" } },
  });
  await page.reload();
  await boot(page);
  const upgraded = await storeKeys(page, "user");
  expect(upgraded, "the release's new stock file seeds into the legacy install").toContain(NEW_STOCK);
  expect(upgraded, "the legacy stock the install already had is untouched").toContain(LEGACY_STOCK);

  // The reader deletes the new thread. Its path is now in the seeded record,
  // so the deletion is final — the next boot must not bring it back.
  await mutate(page, { user: { drop: [NEW_STOCK] } });
  await page.reload();
  await boot(page);
  expect(await storeKeys(page, "user"), "a deleted stock file stays deleted").not.toContain(NEW_STOCK);
});
