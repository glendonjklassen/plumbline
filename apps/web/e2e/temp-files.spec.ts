import { expect, test, type Page } from "@playwright/test";

// STRANDED TEMP FILES ARE NOT THE READER'S DATA, AND MUST NOT TRAVEL AS IF THEY
// WERE.
//
// The core writes a file by writing a hidden sibling — `.<name>.<digits>.tmp` —
// and renaming it over the target. A session killed in between leaves that
// sibling behind in an authored dir, and Android does the same thing, so its
// backup zips can carry one over here on a restore. Nothing filtered them:
// `collectFiles` walked every File under the user dirs, so a temp was persisted
// to IndexedDB as if it were reader data and shipped inside the backup zip,
// where the next device would restore it as a permanent fixture nothing ever
// removes.
//
// The filter is in `collectFiles` (engine/home.ts), the one gate both the
// persists and `exportUserData` pass through. The whole difficulty is the RULE:
// `.config` is a legitimate authored directory (the reader's settings live in
// it) and `config.json.bad` is the deliberate rescue copy of damaged settings,
// which must keep riding along in backups. So "skip anything starting with a
// dot" and "skip anything ending in .tmp" both delete real reader data — which
// is why the negatives below are asserted as hard as the positive.
//
// Seeded through IndexedDB rather than by killing a session mid-write: what is
// under test is what the walk does with a name, and IndexedDB is exactly where a
// stranded temp arrives from (a previous session, or a restored Android backup).
//
// Mutation-tested 2026-07-29 — see the notes at the bottom for the three
// mutations and the assertions that went red.

test.setTimeout(240_000); // two engine boots

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

const THREAD = (name: string) =>
  JSON.stringify({
    format: "overlay-thread-v1",
    name,
    tokenization: "kjv1769-tok2",
    notes: "",
    entries: [],
    created: "2026-07-29T00:00:00Z",
  });

/** What a killed write leaves behind, beside the file it was replacing. */
const STRANDED = "threads/.romans-road.json.4242.tmp";
/** Names a looser rule would take, and the reader would never get back. */
const KEPT = {
  thread: "threads/romans-road.json",
  dotted: "threads/.mine.json", // dotted, but not ours — no .tmp
  tmpish: "threads/summer.tmp", // ends in .tmp, but not dotted
  rescue: ".config/plumbline/config.json.bad", // damaged settings, set aside
};

/** Put bytes straight into the user store, the way a previous session (or a
 *  restore) would have left them. The database already exists — the app booted
 *  once before this runs — so no upgrade path is needed. */
function seedUserFiles(page: Page, files: Record<string, string>): Promise<void> {
  return page.evaluate(async (entries) => {
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = indexedDB.open("plumbline", 1);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const tx = db.transaction("user", "readwrite");
    const enc = new TextEncoder();
    for (const [path, body] of Object.entries(entries)) tx.objectStore("user").put(enc.encode(body), path);
    await new Promise<void>((res, rej) => {
      tx.oncomplete = () => res();
      tx.onerror = () => rej(tx.error);
    });
    db.close();
  }, files);
}

/** Every key in the user store. */
function userKeys(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
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
    return keys.map(String);
  });
}

test("a stranded temp file never reaches a backup, and every dotted file the reader owns does", async ({
  page,
}) => {
  // First boot writes the reader's own config, so `.config/plumbline/config.json`
  // below is the real thing rather than a fixture.
  await boot(page);
  await expect
    .poll(async () => (await userKeys(page)).includes(".config/plumbline/config.json"), {
      timeout: 20_000,
    })
    .toBe(true);

  await seedUserFiles(page, {
    [STRANDED]: THREAD("Stranded copy"),
    [KEPT.thread]: THREAD("Romans Road"),
    [KEPT.dotted]: THREAD("Mine"),
    [KEPT.tmpish]: "not ours to judge",
    [KEPT.rescue]: "{ not json",
  });

  // Second boot restores all five into the home — this is the state the filter
  // has to be right about.
  await page.reload();
  await boot(page);

  // THE BACKUP ZIP IS BUILT FROM exportUserData (see SettingsDialog.backup), so
  // this is the archive's file list.
  const exported = await page.evaluate(async () =>
    [...(await (window as any).__plumbline.rpc.exportUserData())].map(([p]: [string]) => p),
  );
  expect(exported, "a stranded temp got into the backup").not.toContain(STRANDED);
  for (const [what, path] of Object.entries(KEPT))
    expect(exported, `the backup dropped the reader's ${what}`).toContain(path);
  expect(exported, "the backup dropped the reader's settings").toContain(
    ".config/plumbline/config.json",
  );

  // And the engine reads the dotted thread as a thread, while the temp beside it
  // stays invisible — the loading half of the same rule.
  const threads = await page.evaluate(async () => {
    const loaded = await (window as any).__plumbline.engine.threads();
    return (loaded?.threads ?? []).map((t: { name: string }) => t.name);
  });
  expect(threads).toContain("Mine");
  expect(threads).toContain("Romans Road");
  expect(threads, "a stranded temp was loaded as a thread").not.toContain("Stranded copy");

  // Now an authoring write, which runs the diffing persist. It sweeps the temp
  // out of IndexedDB — deliberate: nothing can read it, and left there it ships
  // in every backup from now on. It must take nothing else with it.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("Gen 1:1", "swept", "2026-07-29T00:00:00Z");
  });
  await expect
    .poll(async () => (await userKeys(page)).some((k) => k.startsWith("notes/")), { timeout: 20_000 })
    .toBe(true);

  // Deterministic from here: the sweep rides the same transaction as the note.
  const keys = await userKeys(page);
  expect(keys, "the stranded temp is still in IndexedDB").not.toContain(STRANDED);
  for (const [what, path] of Object.entries(KEPT))
    expect(keys, `the persist deleted the reader's ${what}`).toContain(path);
  expect(keys, "the persist deleted the reader's settings").toContain(
    ".config/plumbline/config.json",
  );
});

// MUTATIONS RUN (engine/home.ts, against a dev server on this source):
//
//  1. Filter removed — `else if (node instanceof File)`, the original bug. Red on
//     "a stranded temp got into the backup": expected list not to contain
//     "threads/.romans-road.json.4242.tmp".
//  2. Widened to `name.startsWith(".")` on files. Red on "the backup dropped the
//     reader's dotted": threads/.mine.json gone from the export.
//  3. Widened to `name.endsWith(".tmp")`. Red on "the backup dropped the reader's
//     tmpish": threads/summer.tmp gone.
//
// The IndexedDB sweep was proved separately, since the export assertion above
// fails first: mutation 1 again with that one line disabled reddens "the
// stranded temp is still in IndexedDB".
//
// The `.config` case cannot be reached by a file-name rule at all — the walk
// recurses into the directory and only ever tests `config.json` — so it is
// pinned in the core instead, where the rule itself is tested
// (crates/core/src/store.rs, `nothing_the_reader_owns_looks_like_a_temp`).
