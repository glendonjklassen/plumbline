import { expect, test, type Browser, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

// The encrypted backup, end to end: the phrase is made and shown once, the file the
// button hands over is an age file, and that file restores on a device that has to be
// told the phrase — which it then keeps. The share sheet is exercised through a stub,
// since headless Chromium has none; what reaches it is checked for being the same
// sealed file a download would have been.
//
// How each would fail against the bug it is for:
//  - a backup that skipped the dialog would not show `[data-surface="phrase"]`, and a
//    dialog that did not gate the file would produce a download before OK is clicked;
//  - a file written in the clear, or as the old plain zip, does not start with the
//    age magic;
//  - a restore that ignored the phrase would not open the prompt after the stored one
//    is removed, and one that accepted a wrong phrase would reload instead of
//    toasting and asking again;
//  - a device that forgot the phrase it was told would have nothing in localStorage
//    after the restore.
// The restored value is read back through the engine (the reader's own note), not
// from storage, as app.spec.ts's zip round-trip does and for the same reason: bytes
// that landed somewhere the engine never reads are not a restore.
//
// "Another device" is a second browser context: its own storage, nothing of its own
// yet. Restore MERGES into a device (core `merge`), so restoring onto the device that
// made the backup proves nothing — everything in the file is already there.

test.setTimeout(240_000); // two boots and a restore reload

const AGE_MAGIC = "age-encryption.org/v1\n";
const PHRASE_KEY = "plumbline:backupPhrase";

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

async function openBackup(page: Page): Promise<void> {
  await page.evaluate(() => ((window as any).__plumbline.showBackup = true));
  await expect(page.locator('[data-surface="backup"]')).toBeVisible();
}

/** Add a note on John 3:16 — what a restore can be seen to bring over, read back
 *  through the engine. */
async function addNote(page: Page, text: string): Promise<void> {
  await page.evaluate(async (t) => {
    const s = (window as any).__plumbline;
    const err = await s.engine.userNoteAdd("John 3:16", t, "2026-10-22T00:00:00Z");
    if (err) throw new Error(err);
  }, text);
  await page.evaluate(() => (window as any).__plumbline.rpc.flush());
}

/** The texts of every note on John 3:16, sorted. */
async function notes(page: Page): Promise<string[]> {
  return page.evaluate(async () =>
    (((await (window as any).__plumbline.engine.userNotes())?.notes ?? []) as any[])
      .filter((n) => n.verse === "John 3:16")
      .map((n) => n.text as string)
      .sort(),
  );
}

/** A second device: a fresh browser context with its own, empty storage. */
async function anotherDevice(browser: Browser): Promise<Page> {
  const ctx = await browser.newContext({ baseURL: (test.info().project.use as any).baseURL });
  const page = await ctx.newPage();
  await boot(page);
  return page;
}

/** Hand a file to the Restore row and wait for the reload it triggers. */
async function restoreFile(page: Page, name: string, buffer: Buffer): Promise<void> {
  await page.evaluate(() => ((window as any).__preRestore = true));
  await page.locator('input[type="file"]').setInputFiles({ name, mimeType: "application/octet-stream", buffer });
}

async function reloaded(page: Page): Promise<void> {
  await expect
    .poll(
      async () => page.evaluate(() => (window as any).__preRestore ?? null).catch(() => "navigating"),
      { timeout: 30_000 },
    )
    .toBeNull();
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

test("the first backup shows the phrase once, and the file restores on a device that has to be told it", async ({
  page,
  browser,
}, testInfo) => {
  await boot(page);
  await addNote(page, "sealed probe");
  expect(await page.evaluate((k) => localStorage.getItem(k), PHRASE_KEY), "fixture: no phrase yet").toBeNull();

  const saved: string[] = [];
  page.on("download", (d) => saved.push(d.suggestedFilename()));

  await openBackup(page);
  await page.getByRole("button", { name: "Secure backup" }).click();

  // The phrase, before any file exists.
  const dialog = page.locator('[data-surface="phrase"]');
  await expect(dialog).toBeVisible();
  const phrase = (await dialog.locator(".words").textContent())!.trim();
  expect(phrase.split(" "), "five words from the list").toHaveLength(5);
  expect(phrase).toMatch(/^[a-z]+( [a-z]+){4}$/);
  expect(saved, "nothing is written before the reader has seen the phrase").toEqual([]);

  const [download] = await Promise.all([
    page.waitForEvent("download"),
    dialog.getByRole("button", { name: "OK" }).click(),
  ]);
  expect(download.suggestedFilename()).toMatch(/^plumbline-backup-\d{4}-\d\d-\d\d\.age$/);
  const agePath = testInfo.outputPath("backup.age");
  await download.saveAs(agePath);
  const file = readFileSync(agePath);
  expect(file.subarray(0, AGE_MAGIC.length).toString("latin1"), "an age file, not a zip").toBe(AGE_MAGIC);
  expect(file.indexOf("plumbline-backup.json"), "the zip's names must not show through").toBe(-1);
  await expect(page.locator(".toast", { hasText: download.suggestedFilename() })).toBeVisible({ timeout: 15_000 });
  expect(await page.evaluate((k) => localStorage.getItem(k), PHRASE_KEY), "this device keeps its phrase").toBe(phrase);

  // A second backup asks nothing: the phrase is this device's now.
  const [again] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Secure backup" }).click(),
  ]);
  expect(again.suggestedFilename()).toMatch(/\.age$/);
  await expect(dialog).toBeHidden();

  // Now another device: no phrase, no notes, the file in hand.
  const other = await anotherDevice(browser);
  expect(await other.evaluate((k) => localStorage.getItem(k), PHRASE_KEY), "fixture: a device with no phrase").toBeNull();
  await openBackup(other);
  await restoreFile(other, "plumbline-backup.age", file);

  // It asks; a wrong phrase is refused and asked again, with the typo left to fix.
  const prompt = other.locator('[role="dialog"][aria-label="Enter passphrase"]');
  await expect(prompt).toBeVisible();
  await prompt.locator("input").fill("harbor pencil olive drum acorn");
  await prompt.getByRole("button", { name: "OK" }).click();
  await expect(other.locator(".toast", { hasText: "Wrong passphrase" })).toBeVisible({ timeout: 15_000 });
  await expect(prompt).toBeVisible();
  await expect(prompt.locator("input")).toHaveValue("harbor pencil olive drum acorn");
  expect(await notes(other), "a refused phrase changes nothing").toEqual([]);

  // The right one, spelled carelessly, restores.
  await prompt.locator("input").fill(`  ${phrase.toUpperCase()} `);
  await prompt.getByRole("button", { name: "OK" }).click();
  await reloaded(other);
  expect(await notes(other), "the note from the backup arrived").toEqual(["sealed probe"]);
  expect(
    await other.evaluate((k) => localStorage.getItem(k), PHRASE_KEY),
    "the device that was told the phrase keeps it, as typed by the engine's spelling",
  ).toBe(phrase);
  await other.context().close();
});

test("where there is a share sheet, the backup goes to it and not to a download", async ({ page }) => {
  // A share sheet that records what it was handed. Chromium on Linux has none, so this
  // is the only way the share path runs here; the stub keeps the real contract — files,
  // a title, an AbortError when the reader closes it.
  await page.addInitScript((key: string) => {
    localStorage.setItem(key, "harbor pencil olive drum acid");
    (navigator as any).canShare = (d: { files?: File[] }) => !!d?.files?.length;
    (navigator as any).share = async (d: { files: File[]; title: string }) => {
      if ((window as any).__declineShare) throw new DOMException("closed", "AbortError");
      const f = d.files[0];
      const bytes = new Uint8Array(await f.arrayBuffer());
      (window as any).__shared = {
        name: f.name,
        title: d.title,
        head: String.fromCharCode(...bytes.subarray(0, 22)),
        size: bytes.length,
      };
    };
  }, PHRASE_KEY);
  const saved: string[] = [];
  page.on("download", (d) => saved.push(d.suggestedFilename()));

  await boot(page);
  await openBackup(page);
  const backUp = page.getByRole("button", { name: "Secure backup" });
  await backUp.click();
  await expect.poll(() => page.evaluate(() => (window as any).__shared ?? null), { timeout: 30_000 }).not.toBeNull();
  const shared = await page.evaluate(() => (window as any).__shared);
  expect(shared.name).toMatch(/^plumbline-backup-\d{4}-\d\d-\d\d\.age$/);
  expect(shared.title).toBe(shared.name);
  expect(shared.head, "the sheet gets the sealed file").toBe(AGE_MAGIC);
  expect(shared.size).toBeGreaterThan(200);
  await expect(page.locator(".toast", { hasText: shared.name })).toBeVisible({ timeout: 15_000 });
  expect(saved, "a shared backup is not also downloaded").toEqual([]);

  // Closing the sheet is neither a backup nor a failure: no toast, no download.
  await page.evaluate(() => {
    (window as any).__shared = null;
    (window as any).__declineShare = true;
  });
  await expect(page.locator(".toast")).toBeHidden({ timeout: 15_000 });
  await backUp.click();
  await page.waitForTimeout(1500);
  await expect(page.locator(".toast")).toBeHidden();
  expect(saved).toEqual([]);
});

test("backing up without a phrase never makes one, and the plain zip restores", async ({ page, browser }, testInfo) => {
  // For the reader who will not keep a phrase. It must not ask for one, make one or
  // remember one: a dialog here, or a phrase in storage afterwards, is the bug.
  await boot(page);
  await addNote(page, "sealed probe");
  await openBackup(page);
  const saved: string[] = [];
  page.on("download", (d) => saved.push(d.suggestedFilename()));
  const plain = page.locator('[data-surface="backup"]').getByRole("button", { name: "Backup", exact: true });
  const confirm = page.locator('[data-surface="confirm"]');

  // It asks first, every time, and Cancel writes nothing.
  await plain.click();
  await expect(confirm, "a plain backup must be confirmed").toBeVisible();
  await expect(confirm).toContainText("Anyone with the file can read it");
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(confirm).toBeHidden();
  await page.waitForTimeout(1000);
  expect(saved, "a cancelled plain backup must not write a file").toEqual([]);

  await plain.click();
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    confirm.getByRole("button", { name: "Back up", exact: true }).click(),
  ]);
  await expect(page.locator('[data-surface="phrase"]'), "no phrase is shown for a plain backup").toHaveCount(0);
  expect(await page.evaluate((k) => localStorage.getItem(k), PHRASE_KEY), "no phrase is made or kept").toBeNull();
  expect(download.suggestedFilename()).toMatch(/^plumbline-backup-\d{4}-\d\d-\d\d\.zip$/);
  const zipPath = testInfo.outputPath("backup.zip");
  await download.saveAs(zipPath);
  const zip = readFileSync(zipPath);
  expect(zip.subarray(0, 2).toString("latin1"), "a zip, in the clear").toBe("PK");

  // Restores on another device with nothing asked: there is no phrase to ask for.
  const other = await anotherDevice(browser);
  await openBackup(other);
  await restoreFile(other, "plumbline-backup.zip", zip);
  await reloaded(other);
  expect(await notes(other)).toEqual(["sealed probe"]);
  await other.context().close();
});

test("restoring one device's backup on another keeps both devices' notes on the same verse", async ({
  page,
  browser,
}, testInfo) => {
  // The point of merging: a phone and a laptop each wrote a note on John 3:16. Restoring
  // the laptop's backup on the phone must leave the phone with BOTH — a restore that
  // wrote the backup's files over the device's would leave one. And restoring the same
  // backup a second time must change nothing.
  await boot(page);
  await addNote(page, "from the phone");

  const laptop = await anotherDevice(browser);
  await addNote(laptop, "from the laptop");
  await openBackup(laptop);
  await laptop.locator('[data-surface="backup"]').getByRole("button", { name: "Backup", exact: true }).click();
  const [download] = await Promise.all([
    laptop.waitForEvent("download"),
    laptop.locator('[data-surface="confirm"]').getByRole("button", { name: "Back up", exact: true }).click(),
  ]);
  const zipPath = testInfo.outputPath("laptop.zip");
  await download.saveAs(zipPath);
  await laptop.context().close();
  const zip = readFileSync(zipPath);

  for (const pass of ["first", "second"]) {
    await openBackup(page);
    await restoreFile(page, "laptop.zip", zip);
    await reloaded(page);
    expect(await notes(page), `after the ${pass} restore`).toEqual(["from the laptop", "from the phone"]);
  }
});



test("Backup is its own item in the menu, not part of Settings", async ({ page }) => {
  await boot(page);
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Backup", exact: true }).click();
  await expect(page.locator('[data-surface="backup"]')).toBeVisible();
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.showBackup = false;
    s.showSettings = true;
  });
  await expect(page.locator('[data-surface="settings"]')).toBeVisible();
  await expect(
    page.locator('[data-surface="settings"]').getByRole("button", { name: "Secure backup" }),
    "backup moved out of Settings",
  ).toHaveCount(0);
});
