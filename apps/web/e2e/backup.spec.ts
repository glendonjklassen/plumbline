import { expect, test, type Page } from "@playwright/test";

// A backup that FAILS must say so.
//
// `backup()` had no error handling at all while `restore()` right below it had
// two layers of it. So a rejection anywhere in the export → zip → save chain
// was an unhandled rejection: the reader tapped "Back up (.zip)" and NOTHING
// happened. Nothing is the worst possible report, because it is exactly what a
// browser that saved the file somewhere they haven't looked also looks like.
//
// The failures are injected at real browser APIs the real code calls, not by
// stubbing the app — the same shape as e2e/restore-failure.spec.ts hooking
// `IDBObjectStore.prototype.put`:
//   * `Worker.prototype.postMessage` refuses the `export` op (DataCloneError),
//     which fails the FIRST await in backup() — the export;
//   * `URL.createObjectURL` refuses the blob, which fails the LAST step, after
//     the zip has been built — so the guard is shown to cover the whole body
//     rather than only the call it wraps most obviously.
// Both are flag-gated and page-realm only, so nothing else in the session is
// touched (the engine worker has its own global scope), and every other worker
// message travels the real path.
//
// The third phase backs up for real. It is what makes the first two mean
// anything: the guard reports failures without swallowing the success, and the
// injected failures were failures rather than a backup that never worked here.
//
// Mutation-tested 2026-07-29 (working rules: break the fix, watch it fail):
//   * the pre-fix body, no try/catch at all → red on "a backup the export
//     refused must tell the reader, not do nothing at all" (the toast is never
//     rendered: "element(s) not found");
//   * the catch kept but the reason dropped → red on "the toast must carry the
//     browser's own words", against the shrug it leaves behind ("unexpected
//     value 'Couldn't make the backup — no file was saved.'");
//   * the guard narrowed to the export call alone → red on "a backup the browser
//     refused to save must be reported too", which is the phase below that
//     exists for exactly that shape of half-fix;
//   * the success toast back to "Backed up N files", unnamed → red on "a backup
//     that worked must say so, and name the file it wrote" (proof the third
//     phase runs, since the three above all stop before it).
// All four restored, green.

test.setTimeout(180_000); // one boot, two refused backups, and one real one

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

test("a backup that cannot be written tells the reader instead of doing nothing", async ({
  page,
}) => {
  // Two refusals the page can turn on, installed before the first navigation.
  // `export` is the only op held back, and `createObjectURL` is called from one
  // place in the whole shell (the backup), so nothing else changes behaviour.
  await page.addInitScript(() => {
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (this: Worker, ...args: unknown[]) {
      const msg = args[0] as { op?: string } | null;
      if ((window as any).__failExport && msg?.op === "export")
        throw new DOMException("the study data could not be cloned (injected)", "DataCloneError");
      return (post as any).apply(this, args);
    } as typeof post;
    const makeUrl = URL.createObjectURL;
    URL.createObjectURL = function (obj: Blob | MediaSource) {
      if ((window as any).__failSave)
        throw new DOMException("no room for the backup file (injected)", "QuotaExceededError");
      return makeUrl.call(URL, obj);
    } as typeof URL.createObjectURL;
  });

  // Every file this page hands the browser. A refused backup must hand it none —
  // otherwise the toast is reporting a failure that didn't happen.
  const saved: string[] = [];
  page.on("download", (d) => saved.push(d.suggestedFilename()));

  await boot(page);
  await page.evaluate(() => ((window as any).__plumbline.showSettings = true));
  const settings = page.locator('[data-surface="settings"]');
  await expect(settings).toBeVisible();
  // Backup lives behind the Advanced disclosure now.
  await settings.locator("details.advanced > summary").click();
  const backUp = settings.getByRole("button", { name: "Back up (.zip)" });

  // ── the export refuses: the first await in backup() ──────────────────────
  await page.evaluate(() => ((window as any).__failExport = true));
  await backUp.click();
  const exportFailed = page.locator(".toast", { hasText: "Couldn't make the backup" });
  await expect(
    exportFailed,
    "a backup the export refused must tell the reader, not do nothing at all",
  ).toBeVisible({ timeout: 15_000 });
  // The browser's own words, not a shrug — this is the sentence a bug report is
  // written from, and the only clue whether to free space or try again.
  await expect(exportFailed, "the toast must carry the browser's own words").toContainText(
    "could not be cloned (injected)",
  );
  expect(saved, "a backup that failed must not also have saved a file").toEqual([]);

  // ── the save refuses: the last step, with the zip already built ──────────
  await page.evaluate(() => {
    (window as any).__failExport = false;
    (window as any).__failSave = true;
  });
  await backUp.click();
  const saveFailed = page.locator(".toast", { hasText: "no room for the backup file (injected)" });
  await expect(
    saveFailed,
    "a backup the browser refused to save must be reported too — the guard covers the whole of backup(), not just the export",
  ).toBeVisible({ timeout: 15_000 });
  expect(saved, "the refused save must not have produced a file").toEqual([]);

  // ── and with nothing refusing, a backup still happens ────────────────────
  await page.evaluate(() => ((window as any).__failSave = false));
  const [download] = await Promise.all([page.waitForEvent("download"), backUp.click()]);
  expect(download.suggestedFilename()).toMatch(/^plumbline-backup-\d{4}-\d\d-\d\d\.zip$/);
  // Named in the message as well, so a reader whose phone was showing them a
  // save dialog knows what file to go and look for.
  await expect(
    page.locator(".toast", { hasText: download.suggestedFilename() }),
    "a backup that worked must say so, and name the file it wrote",
  ).toBeVisible({ timeout: 15_000 });
});
