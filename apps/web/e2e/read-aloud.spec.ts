import { expect, test, type Page } from "@playwright/test";

// Read-aloud (maintainer ask, 2026-08-24): the verse menu offers "Read aloud
// from here" (this verse to the chapter's end) and "Read chapter aloud", over
// the Web Speech API — one utterance PER VERSE, so stopping lands between
// verses. A sticky chip is the only sign a voice is running and the only off
// switch; ✕ cancels the queue.
//
// The synthesis engine is STUBBED before the app loads: headless browsers ship
// no voices, and the subject here is what the app hands the synthesizer — the
// verses, their order, their language — not the sound card.

async function boot(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const spoken: any[] = [];
    (window as any).__spoken = spoken;
    (window as any).__cancels = 0;
    // defineProperty, not assignment: `speechSynthesis` is a readonly accessor
    // on Window, so a plain `window.speechSynthesis = …` is a silent no-op and
    // the REAL synthesizer throws on our stub utterances.
    Object.defineProperty(window, "speechSynthesis", {
      configurable: true,
      value: {
        cancel: () => ((window as any).__cancels += 1),
        speak: (u: any) => spoken.push({ text: u.text, lang: u.lang }),
      },
    });
    (window as any).SpeechSynthesisUtterance = class {
      text: string;
      lang = "";
      onend: (() => void) | null = null;
      onerror: (() => void) | null = null;
      constructor(t: string) {
        this.text = t;
      }
    };
  });
  await page.setViewportSize({ width: 1100, height: 800 });
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

const openMenuAt = (page: Page, refKey: string): Promise<void> =>
  page.evaluate((ref) => {
    (window as any).__plumbline.contextMenu = { x: 40, y: 120, refKey: ref };
  }, refKey);

test("read from here queues this verse to the chapter's end, and ✕ stops it", async ({ page }) => {
  await boot(page);

  // John 3 has 36 verses; from verse 34 the queue is exactly 34, 35, 36.
  await openMenuAt(page, "John 3:34");
  await page.getByRole("button", { name: "Read aloud from here" }).click();

  await expect.poll(() => page.evaluate(() => (window as any).__spoken.length)).toBe(3);
  const first = await page.evaluate(() => (window as any).__spoken[0]);
  // The verse's own words reach the synthesizer — John 3:34 opens so.
  expect(first.text).toContain("For he whom God hath sent");
  expect(first.lang).toBe("en");

  // The chip is up, names the passage, and its ✕ cancels the queue.
  const chip = page.locator(".toast", { hasText: "Reading aloud" });
  await expect(chip).toBeVisible();
  await expect(chip).toContainText("John 3:34");
  const cancelsBefore = await page.evaluate(() => (window as any).__cancels);
  await chip.getByRole("button", { name: "Stop reading" }).click();
  await expect(chip).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__cancels)).toBeGreaterThan(cancelsBefore);
});

test("read chapter aloud queues every verse from the first", async ({ page }) => {
  await boot(page);

  await openMenuAt(page, "John 3:34");
  await page.getByRole("button", { name: "Read chapter aloud" }).click();

  await expect.poll(() => page.evaluate(() => (window as any).__spoken.length)).toBe(36);
  const first = await page.evaluate(() => (window as any).__spoken[0]);
  // Verse 1, not the verse the menu was opened on.
  expect(first.text).toContain("Nicodemus");
  // The chip names the chapter, not the verse the menu happened to open on.
  await expect(page.locator(".toast", { hasText: "Reading aloud" })).toContainText("John 3");
});
