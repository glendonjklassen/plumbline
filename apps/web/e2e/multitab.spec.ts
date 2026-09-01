import { expect, test, type Page } from "@playwright/test";

// Two tabs, one IndexedDB. `persistUserData` used to rewrite the WHOLE user
// subtree from the tab's boot-time snapshot on every authoring write, so the
// slower tab's stale copies overwrote the faster tab's edits, and a file
// deleted in one tab was resurrected by the other tab's next write — silently,
// on the reader's own notes. The fix (engine/home.ts): a tab persists only
// files whose bytes differ from what IT last synced, and deletes only files IT
// removed. Editing the SAME file in two tabs stays last-writer-wins by design;
// what this pins is that a tab which never touched a file can no longer
// destroy it.
//
// The assertions read IndexedDB by value, not by key: an edit reuses its key,
// so key-existence polling (reading.spec.ts's waitForPersist) would pass before
// the write it waits for. The clobber and the resurrection would ride the SAME
// transaction as the probe write they follow, so once the probe is visible the
// verdict is deterministic.
//
// Mutation-tested 2026-07-29 (working rules: break the fix, watch it fail):
// with the pre-fix home.ts (`git show <base>:apps/web/src/engine/home.ts`)
// swapped in and REBUILT — the suite runs against dist/ — this test goes red
// on the deterministic assertions ("first draft" back in the store, the
// deleted card resurrected). Restored, green.

test.setTimeout(240_000); // three engine boots share this test

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** How many user-store VALUES contain `needle` (decoded as UTF-8). */
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

/** How many user-store KEYS start with `prefix`. */
function userKeysWithPrefix(page: Page, prefix: string): Promise<number> {
  return page.evaluate(async (p) => {
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = indexedDB.open("plumbline", 1);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const keys = await new Promise<IDBValidKey[]>((res) => {
      const q = db.transaction("user").objectStore("user").getAllKeys();
      q.onsuccess = () => res(q.result);
      q.onerror = () => res([]);
    });
    db.close();
    return keys.filter((k) => String(k).startsWith(p)).length;
  }, prefix);
}

test("a write in one tab neither clobbers another tab's edits nor resurrects its deletions", async ({
  page,
  context,
}) => {
  // Tab one authors a note and a memory card; both reach IndexedDB.
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("Gen 1:1", "first draft", "2026-07-29T00:00:00Z");
    await s.engine.memoryAdd("Gen 2:5", "2026-07-29T00:00:00Z");
  });
  await expect
    .poll(() => userValuesContaining(page, "first draft"), { timeout: 20_000 })
    .toBeGreaterThan(0);
  await expect.poll(() => userKeysWithPrefix(page, "memory/"), { timeout: 20_000 }).toBeGreaterThan(0);

  // Tab two boots over the same IndexedDB — its snapshot includes both files —
  // then edits the note and deletes the card.
  const two = await context.newPage();
  await boot(two);
  await two.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("Gen 1:1", "edited in tab two", "2026-07-29T01:00:00Z");
    await s.engine.memoryRemove("Gen 2:5");
  });
  await expect
    .poll(() => userValuesContaining(two, "edited in tab two"), { timeout: 20_000 })
    .toBeGreaterThan(0);
  await expect.poll(() => userKeysWithPrefix(two, "memory/"), { timeout: 20_000 }).toBe(0);

  // Tab one — whose in-memory home still holds "first draft" AND the card —
  // authors something unrelated. Its persist must carry only that.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("John 3:16", "note from tab one", "2026-07-29T02:00:00Z");
  });
  await expect
    .poll(() => userValuesContaining(page, "note from tab one"), { timeout: 20_000 })
    .toBeGreaterThan(0);

  // Deterministic now: the stale rewrite would have shared that transaction.
  expect(await userValuesContaining(page, "edited in tab two")).toBeGreaterThan(0);
  expect(await userValuesContaining(page, "first draft")).toBe(0);
  expect(await userKeysWithPrefix(page, "memory/")).toBe(0);

  // And a fresh session reads the surviving truth back through the engine.
  await two.close();
  const three = await context.newPage();
  await boot(three);
  const state = await three.evaluate(async () => {
    const s = (window as any).__plumbline;
    return {
      gen: (await s.engine.userNote("Gen 1:1"))?.text,
      john: (await s.engine.userNote("John 3:16"))?.text,
      card: await s.engine.memoryCard("Gen 2:5"),
    };
  });
  expect(state.gen).toBe("edited in tab two");
  expect(state.john).toBe("note from tab one");
  expect(state.card).toBeNull();
});
