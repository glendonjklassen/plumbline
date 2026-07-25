import { expect, test, type Page } from "@playwright/test";

// Boot the app and wait for the reader. First-run modal is dismissed via
// "Start reading" (fresh storage per test).
async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".subtitle")).toContainText("1769 KJV", { timeout: 90_000 });
  const start = page.getByRole("button", { name: "Start reading" });
  if (await start.isVisible().catch(() => false)) await start.click();
}

test("boots to the reader with the stock set seeded", async ({ page }) => {
  await boot(page);
  await expect(page.locator("canvas").first()).toBeVisible();
  const counts = await page.evaluate(() => {
    const s = (window as any).__pureStudy;
    return {
      weaves: s.engine.weaves()?.weaves?.length ?? 0,
      threads: s.engine.threads()?.threads?.length ?? 0,
      tags: s.engine.tags()?.tags?.length ?? 0,
    };
  });
  expect(counts.weaves).toBeGreaterThan(20);
  expect(counts.threads).toBeGreaterThanOrEqual(1);
  expect(counts.tags).toBeGreaterThanOrEqual(4);
});

test("menus open promptly after boot (freeze regression)", async ({ page }) => {
  await boot(page);
  // The analytics warm-up must happen behind the splash — if it leaks past
  // boot, this click stalls for seconds and the assertion times out.
  const t0 = Date.now();
  await page.getByLabel("Menu").click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 2_000 });
  expect(Date.now() - t0).toBeLessThan(2_000);
});

test("menu destinations are exclusive (memorize does not linger)", async ({ page }) => {
  await boot(page);
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Memorize" }).click();
  await expect(page.getByText("Review due")).toBeVisible();
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Explore", exact: true }).click();
  await expect(page.getByText("Review due")).toBeHidden();
  await expect(page.getByText("Weave map")).toBeVisible();
});

test("word study opens from a double-click and respects the gates", async ({ page }) => {
  await boot(page);
  const canvas = page.locator("canvas").first();
  const box = (await canvas.boundingBox())!;
  // Walk the first text line until a word hit opens the panel.
  for (const x of [0.3, 0.35, 0.4, 0.45, 0.5]) {
    await canvas.dblclick({ position: { x: box.width * x, y: 46 } });
    if (await page.locator("aside.panel").isVisible().catch(() => false)) break;
  }
  await expect(page.locator("aside.panel")).toBeVisible();
  await expect(page.locator("aside.panel").getByText("your note")).toBeVisible();
});

test("live search shows results and Esc clears", async ({ page }) => {
  await boot(page);
  await page.getByLabel("Search").fill("in the beginning");
  await expect(page.locator("aside.panel")).toContainText("result");
  await page.keyboard.press("Escape");
  await expect(page.locator("aside.panel")).toBeHidden();
});

test("settings switch the theme", async ({ page }) => {
  await boot(page);
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByText("Night (true black)").click();
  const paper = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue("--paper").trim(),
  );
  expect(paper.toLowerCase()).toContain("#0");
});

test("passage navigator jumps to a verse", async ({ page }) => {
  await boot(page);
  await page.locator(".nav .passage").first().click();
  await page.getByRole("button", { name: "Genesis", exact: true }).click();
  await page.getByRole("button", { name: "15", exact: true }).click();
  await page.getByRole("button", { name: "6", exact: true }).click();
  await expect(page.locator(".subtitle")).toContainText("Gen 15");
});

test("backup round-trips through a zip", async ({ page }, testInfo) => {
  await boot(page);
  await page.evaluate(() => {
    const s = (window as any).__pureStudy;
    s.engine.userNoteSet("John 3:16", "backup probe", "2026-07-25T00:00:00Z");
  });
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Back up (.zip)" }).click(),
  ]);
  const zipPath = testInfo.outputPath("backup.zip");
  await download.saveAs(zipPath);

  // Damage the note, then restore the backup over it.
  await page.evaluate(() => {
    const s = (window as any).__pureStudy;
    s.engine.userNoteSet("John 3:16", "damaged", "2026-07-25T01:00:00Z");
  });
  // Mark the current document, then wait until the restore's reload has
  // actually replaced it (waitForLoadState resolves against the old page).
  await page.evaluate(() => ((window as any).__preRestore = true));
  await page.locator('input[type="file"]').setInputFiles(zipPath);
  await expect
    .poll(async () => page.evaluate(() => (window as any).__preRestore ?? null), {
      timeout: 30_000,
    })
    .toBeNull();
  await expect(page.locator(".subtitle")).toContainText("1769 KJV", { timeout: 90_000 });
  const text = await page.evaluate(
    () => (window as any).__pureStudy.engine.userNote("John 3:16")?.text,
  );
  expect(text).toBe("backup probe");
});
