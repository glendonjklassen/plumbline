import { expect, test, type Page } from "@playwright/test";

import { zipWrite } from "../src/engine/zip";

// A restore that FAILS must not take the session down with it.
//
// Restoring mutes this session's writes before it touches IndexedDB — on
// purpose: `s.restoring` stops the config persist and `home.freeze()` stops the
// authoring persist, so nothing can save the old session over the restored
// files. Neither has an undo. So the failure path used to end the session's
// useful life: a 2.2 s toast, no reload, and from then on every note, tag,
// thread and setting appeared to save and silently went nowhere. The reader's
// only way out was a reload nobody had told them to do.
//
// The load-bearing assertion here is NOT that an error appeared. It is that the
// session can still SAVE afterwards — read straight out of IndexedDB, the way
// multitab.spec.ts does, because that is the thing the reader loses.
//
// The failure is injected at the browser API, not at the app: the page's own
// `IDBObjectStore.put` refuses writes to the "user" store while a flag is set,
// which is what a device out of quota does. Nothing stubs `idbApply`, so the
// rejection travels the real path. The hook is page-context only — the engine
// worker has its own global scope, so the authoring persist that this test
// depends on is untouched by it.
//
// Mutation-tested 2026-07-29 (working rules: break the fix, watch it fail):
//   * the pre-fix catch (`s.showToast(...)`, no reload) → red on the persistence
//     poll, "authoring after a failed restore must reach IndexedDB — a session
//     left frozen accepts every write and keeps none";
//   * the reload kept, the carried message dropped → red on "a failed restore is
//     reported blocking, not as a 2.2 s toast".
// Both restored, green.

test.setTimeout(240_000); // a boot, a failed restore, and the reload after it

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** Every value in the user store, decoded — the durable truth about this home. */
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
  // Refuse the page's writes to the user store on demand. Installed before the
  // first navigation so it survives the reload the fix performs (each document
  // gets a fresh realm, and a fresh flag with it — which is exactly right: the
  // session on the other side must be able to write again).
  await page.addInitScript(() => {
    const put = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (this: IDBObjectStore, ...args: unknown[]) {
      if ((window as any).__failUserWrites && this.name === "user")
        throw new DOMException("no space left on device (injected)", "QuotaExceededError");
      return (put as any).apply(this, args);
    } as typeof put;
  });

  await boot(page);

  // The reload must land back on the reader rather than the first-run chooser,
  // so wait until this session's config is actually durable.
  await expect
    .poll(async () => (await userStore(page)).some((e) => e.startsWith(".config/")), {
      timeout: 30_000,
      message: "the config should reach IndexedDB before we reload the page",
    })
    .toBe(true);

  await page.evaluate(() => ((window as any).__plumbline.showSettings = true));
  const settings = page.locator('[data-surface="settings"]');
  await expect(settings).toBeVisible();

  // A window marker only a real document navigation can clear, so "it reloaded"
  // is asserted rather than assumed (sessionStorage would survive; this cannot).
  await page.evaluate(() => {
    (window as any).__beforeRestore = true;
    (window as any).__failUserWrites = true;
  });
  await settings.locator('input[type="file"]').setInputFiles({
    name: "plumbline-backup-2026-07-29.zip",
    mimeType: "application/zip",
    buffer: backupZip(),
  });

  // Whichever way it reports, the report is what settles the page: the fix
  // reloads and shows the notice on the other side; the bug toasts in place.
  // Swallowed, so the assertions below are what fail rather than this wait.
  const notice = page.locator('[data-surface="restore-failed"], .toast');
  await notice
    .first()
    .waitFor({ state: "visible", timeout: 90_000 })
    .catch(() => {});

  // The restore really did fail — nothing out of the zip is in the store.
  expect(
    (await userStore(page)).filter((e) => e.includes("From the backup")),
    "the injected failure must actually have stopped the restore",
  ).toEqual([]);

  // ── the load-bearing part: this session can still save ──────────────────
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

  // Because it can save: the session was replaced rather than left standing
  // with its writes muted.
  expect(
    await page.evaluate(() => (window as any).__beforeRestore === true),
    "a failed restore must reload the page — the freeze it took out has no undo",
  ).toBe(false);

  // And the reader was told, in something they cannot miss by looking away.
  const blocking = page.locator('[data-surface="restore-failed"]');
  await expect(blocking, "a failed restore is reported blocking, not as a 2.2 s toast").toBeVisible();
  await expect(blocking).toContainText("nothing changed");
  // Nothing was half-applied: one IndexedDB transaction, rolled back whole.
  await expect(blocking).toContainText("your own study data is as it was");

  // It closes on the reader's word, and only theirs.
  await page.locator('[data-surface="restore-failed"] button').click();
  await expect(blocking).toBeHidden();
});
