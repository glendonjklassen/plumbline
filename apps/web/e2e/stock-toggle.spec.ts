import { expect, test, type Page } from "@playwright/test";

// Turning the bundled study set OFF used to delete every stock PATH out of
// IndexedDB, so a stock thread, tag or weave the reader had renamed, re-noted or
// added verses to was destroyed along with the untouched ones — their own work,
// erased by a settings toggle that reads as "hide the examples" (2026-07-29).
//
// The fix in engine/home.ts: `setBundled(false)` removes a stock path only when
// its bytes are EXACTLY the bundled file's, in the live home AND in IndexedDB.
// Their copy wins on the way in (buildHome lays saved files over freshly-seeded
// stock); this is the same invariant on the way out.
//
// The assertions read IndexedDB by VALUE as well as by key, because the failure
// this pins keeps the key count identical to the pristine case — a key-existence
// check alone would pass with the reader's edit already gone.
//
// Mutation-tested 2026-07-29: with `await idbApply("user", new Map(), [...stockPaths])`
// back in place of the pristine check, this goes red on "the reader's edit to the
// stock thread was deleted" — the key is simply absent. Restored, green.

test.setTimeout(240_000); // two engine boots share this test

const STOCK_THREAD = "threads/romans-road.json";
const STOCK_TAG = "tags/false-teaching.json";
/** What the reader types into the stock thread's notes. Any bytes will do — the
 *  rule under test is byte equality, not meaning. */
const MINE = "kept by the reader";

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** The user store's study files: home-relative path → its bytes as text. Only
 *  the three authored dirs the stock set lives in, so a reader's notes and
 *  reading map stay out of the comparison. */
function studyFiles(page: Page): Promise<Record<string, string>> {
  return page.evaluate(async () => {
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = indexedDB.open("plumbline", 1);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const [keys, values] = await Promise.all([
      new Promise<IDBValidKey[]>((res) => {
        const q = db.transaction("user").objectStore("user").getAllKeys();
        q.onsuccess = () => res(q.result);
        q.onerror = () => res([]);
      }),
      new Promise<unknown[]>((res) => {
        const q = db.transaction("user").objectStore("user").getAll();
        q.onsuccess = () => res(q.result);
        q.onerror = () => res([]);
      }),
    ]);
    db.close();
    const dec = new TextDecoder();
    const out: Record<string, string> = {};
    keys.forEach((k, i) => {
      const path = String(k);
      if (/^(threads|tags|weaves)\//.test(path)) out[path] = dec.decode(values[i] as Uint8Array);
    });
    return out;
  });
}

const countUnder = (files: Record<string, string>, dir: string) =>
  Object.keys(files).filter((k) => k.startsWith(`${dir}/`)).length;

/** Edit a stored stock file IN INDEXEDDB ONLY, changing exactly one byte and
 *  keeping the length — what a SECOND TAB's edit looks like from here.
 *
 *  Two shortcuts die on this fixture and both would cost real data: a rule that
 *  compares byte COUNTS, and a rule that only looks at this tab's live home (the
 *  other tab wrote after our boot, so our tree still holds the pristine copy).
 *  The byte moved is a digit of `created`, so the file stays valid JSON and the
 *  engine can still load it after the reload. */
function editStoredByOneByte(page: Page, path: string): Promise<string> {
  return page.evaluate(async (p) => {
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = indexedDB.open("plumbline", 1);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const before = await new Promise<Uint8Array>((res, rej) => {
      const q = db.transaction("user").objectStore("user").get(p);
      q.onsuccess = () => res(q.result as Uint8Array);
      q.onerror = () => rej(q.error);
    });
    const text = new TextDecoder().decode(before);
    const key = '"created":"';
    const at = text.indexOf(key) + key.length + 9; // the last digit of YYYY-MM-DD
    if (!text.includes(key)) throw new Error(`no created stamp in ${p}`);
    const swapped = text[at] === "1" ? "2" : "1";
    const after = text.slice(0, at) + swapped + text.slice(at + 1);
    if (after.length !== text.length) throw new Error("the fixture must keep the byte count");
    if (after === text) throw new Error("the fixture must change a byte");
    await new Promise<void>((res, rej) => {
      const tx = db.transaction("user", "readwrite");
      tx.objectStore("user").put(new TextEncoder().encode(after), p);
      tx.oncomplete = () => res();
      tx.onerror = () => rej(tx.error);
    });
    db.close();
    return after;
  }, path);
}

test("turning the bundled study set off keeps the reader's edits and takes only the untouched examples", async ({
  page,
}) => {
  await boot(page);

  // The stock set seeded on this first run.
  const seeded = await studyFiles(page);
  expect(seeded[STOCK_THREAD], "the stock thread should have seeded").toBeTruthy();
  expect(seeded[STOCK_TAG], "the stock tag should have seeded").toBeTruthy();
  expect(countUnder(seeded, "weaves"), "the stock weaves should have seeded").toBeGreaterThan(20);
  const stockWeaves = Object.keys(seeded)
    .filter((k) => k.startsWith("weaves/"))
    .sort();
  const [otherTabWeave, pristineWeave] = stockWeaves;

  // The reader makes the stock thread their own, and authors a thread of their
  // own under a name the stock set does not use.
  const err = await page.evaluate(async (mine) => {
    const s = (window as any).__plumbline;
    return (
      (await s.engine.threadSetNotes("Romans Road", mine)) ??
      (await s.engine.threadAdd("Kept by me", "Gen 1:1", null, "2026-07-29T00:00:00Z"))
    );
  }, MINE);
  expect(err, "the authoring calls must succeed").toBeNull();

  // Wait for BOTH to reach IndexedDB — the rule compares the stored copy too, so
  // a test that toggled before the persist would be testing the other branch.
  await expect
    .poll(() => studyFiles(page).then((f) => f[STOCK_THREAD] ?? ""), { timeout: 30_000 })
    .toContain(MINE);
  await expect
    .poll(() => studyFiles(page).then((f) => countUnder(f, "threads")), { timeout: 30_000 })
    .toBe(2);

  // A one-byte, same-length edit to a stock weave, in the store only.
  const otherTabBytes = await editStoredByOneByte(page, otherTabWeave);

  // Hide the examples.
  await page.evaluate(() => (window as any).__plumbline.rpc.setBundled(false));

  const after = await studyFiles(page);
  expect(after[STOCK_THREAD] ?? "", "the reader's edit to the stock thread was deleted").toContain(MINE);
  expect(countUnder(after, "threads"), "their own thread must be there too, untouched").toBe(2);
  expect(
    after[otherTabWeave],
    `a one-byte, same-length edit to ${otherTabWeave} was deleted — the rule compared counts, or only this tab's copy`,
  ).toBe(otherTabBytes);
  expect(after[pristineWeave], `an untouched stock weave (${pristineWeave}) should be gone`).toBeUndefined();
  expect(countUnder(after, "weaves"), "only the edited stock weave should be left").toBe(1);
  expect(after[STOCK_TAG], "the untouched stock tag should be gone").toBeUndefined();

  // And the engine agrees on the next launch, which is what the reader sees.
  await page.reload();
  await boot(page);
  const state = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const threads: any[] = (await s.engine.threads()).threads ?? [];
    const weaves: any[] = (await s.engine.weaves()).weaves ?? [];
    const tags: any[] = (await s.engine.tags()).tags ?? [];
    return {
      names: threads.map((t) => t.name).sort(),
      notes: threads.map((t) => t.notes ?? "").join("\n"),
      weaves: weaves.length,
      tags: tags.length,
    };
  });
  expect(state.names, "the edited stock thread and the reader's own both survive a relaunch").toEqual([
    "Kept by me",
    "Romans Road",
  ]);
  expect(state.notes, "and the edit is still in it").toContain(MINE);
  expect(state.weaves, "the kept weave loads; the untouched ones stay gone").toBe(1);
  expect(state.tags, "the untouched stock tag stays gone").toBe(0);
});
