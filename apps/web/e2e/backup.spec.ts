import { expect, test, type Page } from "@playwright/test";

// Fails against a `backup()` with no error handling: a rejection anywhere in the export → zip →
// save chain did nothing visible, which looks exactly like a file saved somewhere the reader has
// not looked.
//
// The failures are injected at the real browser APIs the code calls, not by stubbing the app:
// `Worker.prototype.postMessage` refuses the `export` op, failing the first await in backup(), and
// `URL.createObjectURL` refuses the blob, failing the last step with the zip already built — so the
// guard is shown to cover the whole body. Both are flag-gated and page-realm only; the engine
// worker has its own global scope. The third phase backs up for real, which is what proves the
// injected failures were failures rather than a backup that never worked here.

test.setTimeout(180_000); // one boot, two refused backups, and one real one

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

test("a backup that cannot be written tells the reader instead of doing nothing", async ({
  page,
}) => {
  // Installed before the first navigation. `export` is the only op held back, and
  // `createObjectURL` has one caller in the shell, so nothing else changes behaviour.
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

  // Every file the page hands the browser: a refused backup must hand it none, or the toast is
  // reporting a failure that did not happen.
  const saved: string[] = [];
  page.on("download", (d) => saved.push(d.suggestedFilename()));

  await boot(page);
  await page.evaluate(() => ((window as any).__plumbline.showSettings = true));
  const settings = page.locator('[data-surface="settings"]');
  await expect(settings).toBeVisible();
  // Backup sits with the everyday settings; there is no Advanced disclosure to open.
  const backUp = settings.getByRole("button", { name: "Back up (.zip)" });

  // The export refuses: the first await in backup().
  await page.evaluate(() => ((window as any).__failExport = true));
  await backUp.click();
  const exportFailed = page.locator(".toast", { hasText: "Couldn't make the backup" });
  await expect(
    exportFailed,
    "a backup the export refused must tell the reader, not do nothing at all",
  ).toBeVisible({ timeout: 15_000 });
  await expect(exportFailed, "the toast must carry the browser's own words").toContainText(
    "could not be cloned (injected)",
  );
  expect(saved, "a backup that failed must not also have saved a file").toEqual([]);

  // The save refuses: the last step, with the zip already built.
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

  // And with nothing refusing, a backup still happens.
  await page.evaluate(() => ((window as any).__failSave = false));
  const [download] = await Promise.all([page.waitForEvent("download"), backUp.click()]);
  expect(download.suggestedFilename()).toMatch(/^plumbline-backup-\d{4}-\d\d-\d\d\.zip$/);
  await expect(
    page.locator(".toast", { hasText: download.suggestedFilename() }),
    "a backup that worked must say so, and name the file it wrote",
  ).toBeVisible({ timeout: 15_000 });
});
