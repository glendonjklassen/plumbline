import { expect, test, type Page } from "@playwright/test";

// Boot the app and wait for the reader. First-run modal is dismissed via
// "Start reading" (fresh storage per test). The title check also pins the
// product branding — index.html, the manifest and the shell header must agree.
async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toContainText("1769 KJV", { timeout: 90_000 });
  const start = page.getByRole("button", { name: "Start reading" });
  if (await start.isVisible().catch(() => false)) await start.click();
}

test("boots to the reader with the stock set seeded", async ({ page }) => {
  await boot(page);
  await expect(page.locator("canvas").first()).toBeVisible();
  const counts = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return {
      weaves: s.engine.weaves()?.weaves?.length ?? 0,
      threads: s.engine.threads()?.threads?.length ?? 0,
      tags: s.engine.tags()?.tags?.length ?? 0,
    };
  });
  expect(counts.weaves).toBeGreaterThan(20);
  expect(counts.threads).toBeGreaterThanOrEqual(1);
  // Tags are the reader's own (semantic) groupings — nothing ships stock.
  // This also guards against stray authoring leftovers in the stock set
  // (a shipped amber highlight once painted John 3:7 on every fresh install).
  expect(counts.tags).toBe(0);
});

test("the deferred machine-tier pack loads after boot", async ({ page }) => {
  await boot(page);
  // Boot ships the core pack only (TODO #28); ensureRnd pulls morphology +
  // concept vectors in and re-warms. Force it (instead of waiting out the
  // idle timer) and check a machine-tier lookup lights up.
  const neighbours = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.ensureRnd();
    return s.engine.conceptNeighbours("G2316", 3)?.near?.length ?? 0; // G2316 = God
  });
  expect(neighbours).toBeGreaterThan(0);
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

test("word study opens from a single click and respects the gates", async ({ page }) => {
  await boot(page);
  const canvas = page.locator("canvas").first();
  const box = (await canvas.boundingBox())!;
  // Walk the first text line until a word hit opens the panel (single click —
  // Compose tap parity, 2026-07-25; the pin/＋link flow is gone).
  for (const x of [0.3, 0.35, 0.4, 0.45, 0.5]) {
    await canvas.click({ position: { x: box.width * x, y: 46 } });
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

test("opening a weave splits to its passages; verse clicks stay responsive (freeze regression)", async ({
  page,
}) => {
  await boot(page);
  await page.getByRole("button", { name: "Weaves", exact: true }).click();
  await expect(page.locator("aside.panel")).toBeVisible();
  // Open the first weave: both endpoint passages must come up on their own.
  await page.locator("aside.panel button.link").first().click();
  await expect(page.locator(".pane canvas")).toHaveCount(2);
  const targets = await page.evaluate(() =>
    (window as any).__plumbline.panes.map((p: any) => ({
      book: p.book,
      chapter: p.chapter,
      verse: p.targetVerse,
    })),
  );
  expect(targets).toHaveLength(2);
  expect(targets[0].verse).not.toBeNull();
  expect(targets[1].verse).not.toBeNull();
  expect(`${targets[0].book} ${targets[0].chapter}`).not.toBe(`${targets[1].book} ${targets[1].chapter}`);

  // Clicking the card's verse links used to spiral the layout effect into an
  // effect_update_depth_exceeded freeze (~10s) that killed reactivity. Each
  // click must settle fast and leave both panes scrollable.
  const verseLink = page.locator("aside.panel button.link", { hasText: /\d+:\d+/ });
  for (const i of [0, 1]) {
    const t0 = Date.now();
    await verseLink.nth(i).click();
    await expect(page.locator(".subtitle")).toContainText("1769 KJV", { timeout: 2_000 });
    expect(Date.now() - t0).toBeLessThan(2_000);
  }
  for (const paneIdx of [0, 1]) {
    const canvas = page.locator(".pane canvas").nth(paneIdx);
    const box = (await canvas.boundingBox())!;
    const before = await page.evaluate(
      (i) => (window as any).__plumbline.panes[i].scrollY,
      paneIdx,
    );
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel(0, before > 0 ? -160 : 160);
    await expect
      .poll(() => page.evaluate((i) => (window as any).__plumbline.panes[i].scrollY, paneIdx))
      .not.toBe(before);
  }
});

test("backup round-trips through a zip", async ({ page }, testInfo) => {
  await boot(page);
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
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
    const s = (window as any).__plumbline;
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
    () => (window as any).__plumbline.engine.userNote("John 3:16")?.text,
  );
  expect(text).toBe("backup probe");
});
