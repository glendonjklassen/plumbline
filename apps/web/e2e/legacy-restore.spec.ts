import { expect, test, type Page } from "@playwright/test";
import { zipWrite } from "../src/engine/zip";

// A backup zip written before the Plumbline rename carries the config under ".config/pure-study/";
// the live home reads ".config/plumbline/", and SettingsDialog.svelte's `currentConfigPath` remaps
// it on the way in. Untested, that shim sits one refactor away from dropping a reader's whole
// config on restore — a failure that does not look like one, since the restore reports success,
// the app reloads, and every setting is quietly back to default.
//
// What this asserts is deliberately not "the key turned up in IndexedDB": a remap that wrote the
// bytes to a path the engine never opens would satisfy that and still lose the settings. So it
// checks what reaches the reader — the chapter that opens, the theme on screen, the text size
// Settings shows back — plus the note the engine answers with for the modern-named entries riding
// along in the same zip, which a shim widened into a general prefix rewrite would break instead.

const enc = new TextEncoder();

/** A config.json as an older build wrote it — the frozen camelCase wire keys, with values a reader
 *  can see rather than flags only a test can. */
const LEGACY_CONFIG = {
  studyMode: "full",
  bodySize: 33,
  theme: "night",
  copyStyle: "verseMarkdown",
  openPanes: [{ book: "Rev", chapter: 22 }],
  activePane: 0,
  sideMargin: 44,
  lineSpacing: 1.8,
  humanAnalysis: true,
  machineAnalysis: false,
};

/** One of the reader's own notes, named the way the store names them (`notes/<slug of the
 *  refKey>.json`) — a modern entry, whose path the shim must leave alone. */
const NOTE = {
  format: "pure-note-v1",
  ref: "John 3:16",
  text: "restored from a pre-rename backup",
  created: "2026-01-01T00:00:00Z",
};

/** The zip an old build handed the reader: the legacy config prefix, a modern
 *  authored file, and the manifest at the root (which restores nothing). */
function legacyBackup(): Buffer {
  const files = new Map<string, Uint8Array>([
    [".config/pure-study/config.json", enc.encode(JSON.stringify(LEGACY_CONFIG))],
    ["notes/john-3-16.json", enc.encode(JSON.stringify(NOTE))],
    ["plumbline-backup.json", enc.encode(JSON.stringify({ format: 1, app: "web", exported: "2026-01-01T00:00:00Z" }))],
  ]);
  return Buffer.from(zipWrite(files));
}

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

async function openSettings(page: Page): Promise<void> {
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.locator('[data-surface="settings"]')).toBeVisible();
}

/** Hand the zip to the Restore row and wait for the reload it triggers. `waitForLoadState`
 *  resolves against the document we already have, so the marker is what tells us the new one
 *  is up. */
async function restore(page: Page, zip: Buffer): Promise<void> {
  await page.evaluate(() => ((window as any).__preRestore = true));
  await page.locator('input[type="file"]').setInputFiles({
    name: "plumbline-backup-2026-01-01.zip",
    mimeType: "application/zip",
    buffer: zip,
  });
  await expect
    // The evaluate can land inside the navigation it is waiting for: the old context is torn
    // down mid-call and it throws, which fails expect.poll rather than retrying it. A destroyed
    // context has to be read as "still navigating", not as null.
    .poll(
      async () => page.evaluate(() => (window as any).__preRestore ?? null).catch(() => "navigating"),
      { timeout: 30_000 },
    )
    .toBeNull();
}

test("a backup written before the rename restores the reader's settings, not the defaults", async ({
  page,
}) => {
  await boot(page);

  // This device's own settings first, so the restore has something to replace and none of the
  // assertions below can pass by accident on a default.
  const before = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { bodySize: Number(s.config.bodySize ?? 18), theme: s.config.theme ?? "system" };
  });
  expect(before.bodySize, "fixture: the live text size must differ from the backup's").not.toBe(33);
  expect(before.theme, "fixture: the live theme must differ from the backup's").not.toBe("night");

  await openSettings(page);
  await restore(page, legacyBackup());
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });

  const after = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return {
      bodySize: Number(s.config.bodySize ?? 18),
      theme: s.config.theme ?? "system",
      lineSpacing: Number(s.config.lineSpacing ?? 1.35),
      sideMargin: Number(s.config.sideMargin ?? 28),
      copyStyle: s.config.copyStyle ?? "verseRef",
      pane: `${s.panes[0].book} ${s.panes[0].chapter}`,
      paper: getComputedStyle(document.documentElement).getPropertyValue("--paper").trim(),
      note: (await s.engine.userNote("John 3:16"))?.text ?? null,
    };
  });

  // The engine opened the restored file: every one of these came out of it.
  const lost =
    "the config from a pre-rename backup did not reach the engine — the reader was told the " +
    "restore worked and got their settings back as defaults";
  expect(after.bodySize, lost).toBe(33);
  expect(after.theme, lost).toBe("night");
  expect(after.lineSpacing, lost).toBe(1.8);
  expect(after.sideMargin, lost).toBe(44);
  expect(after.copyStyle, lost).toBe("verseMarkdown");

  // And the app is using it, not merely holding it: the pane the backup was last in is what
  // opened, and the theme it chose is what paints.
  expect(after.pane, "the restored session opened somewhere else entirely").toBe("Rev 22");
  await expect(page.locator(".subtitle")).toHaveText("Revelation 22", { timeout: 30_000 });
  expect(after.paper.toLowerCase(), "the restored night theme is not on the page").toContain("#0");

  // Settings shows the restored size back to the reader: the Aa preview is rendered at it, so the
  // same number is arrived at through the DOM.
  await openSettings(page);
  const aa = await page.locator(".dialog .aa").evaluate((el) => getComputedStyle(el).fontSize);
  expect(aa, "Settings is not showing the restored text size").toBe("33px");

  // The modern-named entries in the same zip are untouched by the shim.
  expect(after.note, "a modern entry beside the legacy one did not restore").toBe(
    "restored from a pre-rename backup",
  );
});

test("the legacy prefix is remapped under .config only, not wherever it appears", async ({ page }) => {
  // The shim is a read shim for one moved directory. A zip that names a
  // ROOT-level "pure-study/" holds nothing of ours, and generalising the remap
  // into "rewrite this prefix anywhere" would start restoring files from paths
  // the vetting never approved. Asserted through the reload rather than through
  // the toast, so it stays true whatever the copy says.
  await boot(page);
  await openSettings(page);
  await page.evaluate(() => ((window as any).__preRestore = true));
  await page.locator('input[type="file"]').setInputFiles({
    name: "plumbline-backup-rootlevel.zip",
    mimeType: "application/zip",
    buffer: Buffer.from(
      zipWrite(new Map([["pure-study/config.json", enc.encode(JSON.stringify(LEGACY_CONFIG))]])),
    ),
  });
  // Nothing was restored, so nothing reloads and the marker survives.
  await page.waitForTimeout(2_000);
  expect(
    await page.evaluate(() => (window as any).__preRestore ?? null),
    "a root-level pure-study/ entry was restored — the remap has become a general prefix rewrite",
  ).toBe(true);
  expect(await page.evaluate(() => Number((window as any).__plumbline.config.bodySize ?? 18))).not.toBe(33);
});
