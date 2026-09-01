import { expect, test, type Page } from "@playwright/test";

import { zipWrite } from "../src/engine/zip";

// Fails against a restore whose failure path left the session standing. Restoring mutes this
// session's writes before it touches IndexedDB — `s.restoring` stops the config persist,
// `home.freeze()` stops the authoring persist — and neither has an undo, so a failed restore used
// to end the session's useful life: a 2.2 s toast, no reload, and from then on every note, tag,
// thread and setting appeared to save and went nowhere.
//
// The load-bearing assertion is not that an error appeared but that the session can still save
// afterwards, read straight out of IndexedDB.
//
// The failure is injected at the browser API rather than at the app: the page's own
// `IDBObjectStore.put` refuses writes to the "user" store while a flag is set, which is what a
// device out of quota does, so the rejection travels the real path. The hook is page-context only
// — the engine worker has its own global scope, so the authoring persist this test depends on is
// untouched by it.

test.setTimeout(240_000); // a boot, a failed restore, and the reload after it

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** Every value in the user store, decoded. */
function userStore(page: Page): Promise<string[]> {
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
    return keys.map((k, i) => `${String(k)}\n${dec.decode(values[i] as Uint8Array)}`);
  });
}

const enc = new TextEncoder();

/** A backup zip that passes the restore filter — a thread the home doesn't have. */
function backupZip(): Buffer {
  return Buffer.from(
    zipWrite(
      new Map<string, Uint8Array>([
        [
          "threads/From the backup.json",
          enc.encode(JSON.stringify({ name: "From the backup", verses: ["John 3:16"] })),
        ],
        ["plumbline-backup.json", enc.encode(JSON.stringify({ format: 1, app: "web" }))],
      ]),
    ),
  );
}

test("a restore that fails leaves a session that can still save", async ({ page }) => {
  // Installed before the first navigation so it survives the reload the fix performs. Each
  // document gets a fresh realm and a fresh flag, so the session on the other side can write.
  await page.addInitScript(() => {
    const put = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (this: IDBObjectStore, ...args: unknown[]) {
      if ((window as any).__failUserWrites && this.name === "user")
        throw new DOMException("no space left on device (injected)", "QuotaExceededError");
      return (put as any).apply(this, args);
    } as typeof put;
  });

  await boot(page);

  // The reload must land back on the reader rather than the first-run chooser, so wait until this
  // session's config is durable.
  await expect
    .poll(async () => (await userStore(page)).some((e) => e.startsWith(".config/")), {
      timeout: 30_000,
      message: "the config should reach IndexedDB before we reload the page",
    })
    .toBe(true);

  await page.evaluate(() => ((window as any).__plumbline.showSettings = true));
  const settings = page.locator('[data-surface="settings"]');
  await expect(settings).toBeVisible();

  // A window marker only a real document navigation can clear, so "it reloaded" is asserted rather
  // than assumed (sessionStorage would survive; this cannot).
  await page.evaluate(() => {
    (window as any).__beforeRestore = true;
    (window as any).__failUserWrites = true;
  });
  await settings.locator('input[type="file"]').setInputFiles({
    name: "plumbline-backup-2026-07-29.zip",
    mimeType: "application/zip",
    buffer: backupZip(),
  });

  // The report is what settles the page, either way: the fix reloads and shows the notice on the
  // other side, the bug toasts in place. Swallowed so the assertions below fail, not this wait.
  const notice = page.locator('[data-surface="restore-failed"], .toast');
  await notice
    .first()
    .waitFor({ state: "visible", timeout: 90_000 })
    .catch(() => {});

  // The restore really did fail: nothing out of the zip is in the store.
  expect(
    (await userStore(page)).filter((e) => e.includes("From the backup")),
    "the injected failure must actually have stopped the restore",
  ).toEqual([]);

  // A frozen session accepts the write, reports no error, and persists nothing.
  await page.evaluate(() =>
    (window as any).__plumbline.author(
      "threadAdd",
      "After the failed restore",
      "John 3:16",
      null,
      new Date().toISOString(),
    ),
  );
  await expect
    .poll(async () => (await userStore(page)).some((e) => e.includes("After the failed restore")), {
      timeout: 30_000,
      message:
        "authoring after a failed restore must reach IndexedDB — a session left frozen accepts every write and keeps none",
    })
    .toBe(true);

  expect(
    await page.evaluate(() => (window as any).__beforeRestore === true),
    "a failed restore must reload the page — the freeze it took out has no undo",
  ).toBe(false);

  const blocking = page.locator('[data-surface="restore-failed"]');
  await expect(blocking, "a failed restore is reported blocking, not as a 2.2 s toast").toBeVisible();
  await expect(blocking).toContainText("nothing changed");
  // Nothing was half-applied: one IndexedDB transaction, rolled back whole.
  await expect(blocking).toContainText("your own study data is as it was");

  // It closes only on the reader's word.
  await page.locator('[data-surface="restore-failed"] button').click();
  await expect(blocking).toBeHidden();
});
