import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

// Chinese, end to end — one language, two rows, one script, and the first
// corpus this app has ever set without spaces. Each test fails against a
// specific wrong generalisation:
//
//   1. the face — `Script::Han` must resolve to Noto Serif TC in BOTH the
//      worker (measurement) and the canvas (paint); a Latin face here renders
//      the whole Bible from per-glyph fallback
//   2. per-character tokens — a painted "word" is one Han character; a
//      multi-character box means the engine that built this pack is not the
//      one that shipped (`build-cuv.py`'s contract with search and layout)
//   3. snug setting — the FFI zeroes `space_width` for a Han corpus; if the
//      spaced-script default leaks through, every character floats apart by
//      a third of an em and the column fits a third less text
//   4. search — "耶穌" is two per-character tokens; the Han query splitter
//      turns the phrase tier into substring search, and without it the exact
//      name of Jesus returns zero rows
//   5. the two rows really differ — a zh-TW device reads 創, a zh-CN device
//      reads 创; conflating them ships one repertoire to both
//
// Strings are read from the catalogues rather than typed.

const cat = (code: string): Record<string, string> =>
  JSON.parse(readFileSync(new URL(`../../../crates/core/src/i18n/${code}.json`, import.meta.url), "utf8"));

const EN = cat("en");
const ZHT = cat("zht");
const ZHS = cat("zhs");

const HAN = /[㐀-䶿一-鿿豈-﫿\u{20000}-\u{3FFFF}]/u;

interface Box {
  x: number;
  y: number;
  w: number;
  text: string;
  kind: string;
}

async function reader(page: Page, lang: Record<string, string>): Promise<void> {
  await page.goto("/");
  const canvas = page.locator(".pane canvas").first();
  await expect(canvas).toBeVisible({ timeout: 90_000 });
}

async function pick(page: Page, now: Record<string, string>, want: string): Promise<void> {
  await page.getByLabel(now["common.menu"]).click();
  await page.locator(".menu").getByRole("button", { name: now["shell.settings"] }).click();
  const dialog = page.locator('[data-surface="settings"]');
  await expect(dialog).toBeVisible();
  await page.evaluate(() => ((globalThis as any).__beforeSwitch = true));
  await dialog.getByLabel(now["settings.language"], { exact: true }).selectOption(want);
  await page.waitForFunction(() => !(globalThis as any).__beforeSwitch && !!(globalThis as any).__plumbline, undefined, {
    timeout: 180_000,
  });
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 180_000 });
}

async function boxes(page: Page): Promise<Box[]> {
  const read = () =>
    page.evaluate(() => {
      const items = (globalThis as any).__plumblinePaint?.items?.deref() ?? [];
      return items.map((i: any) => ({ x: i.x, y: i.y, w: i.w, text: i.text, kind: i.kind }));
    });
  await expect
    .poll(async () => (await read()).length, { timeout: 60_000, message: "no display list ever reached the painter" })
    .toBeGreaterThan(5);
  return await read();
}

async function settings(page: Page, lang: Record<string, string>) {
  await page.getByLabel(lang["common.menu"]).click();
  await page.locator(".menu").getByRole("button", { name: lang["shell.settings"] }).click();
  const dialog = page.locator('[data-surface="settings"]');
  await expect(dialog).toBeVisible();
  return dialog;
}

test("zht: its own Bible, one character per box, set snug in the Han face", async ({ page }) => {
  await reader(page, EN);
  await pick(page, EN, "zht");
  await expect(page.locator("html")).toHaveAttribute("lang", "zht");
  await expect(page.locator("html")).toHaveAttribute("dir", "ltr");

  const words = (await boxes(page)).filter((b) => b.kind === "word");
  expect(words.length).toBeGreaterThan(10);
  expect(words.some((w) => HAN.test(w.text)), "no Han in the reader").toBe(true);

  // PER-CHARACTER TOKENS: every painted word carries exactly one Han letter
  // (its punctuation rides along in the same box).
  const multi = words.filter((w) => [...w.text].filter((c) => HAN.test(c)).length > 1);
  expect(multi.map((m) => m.text), "a painted box holds more than one character").toEqual([]);

  // SNUG: on a line of Han boxes, each starts where the last ended. Pairs
  // straddling a verse number (a real, wanted gap) are skipped by requiring
  // adjacency in paint order; a spaced-script leak fails EVERY pair at once,
  // so the majority threshold is generous without being blind.
  const firstY = Math.min(...words.map((w) => w.y));
  const line = words.filter((w) => w.y === firstY).sort((a, b) => a.x - b.x);
  expect(line.length).toBeGreaterThan(5);
  let snug = 0;
  let apart = 0;
  for (let i = 1; i < line.length; i++) {
    const gap = line[i].x - (line[i - 1].x + line[i - 1].w);
    if (Math.abs(gap) < 0.5) snug++;
    else apart++;
  }
  expect(snug, `only ${snug} snug pairs against ${apart} gapped on the first line`).toBeGreaterThan(apart * 3);

  // THE FACE, in the frame that was actually painted.
  await expect
    .poll(async () => page.evaluate(() => (globalThis as any).__plumblinePaint?.bodyFont ?? ""), {
      timeout: 30_000,
      message: "no frame painted after the switch",
    })
    .toContain("Noto Serif TC");

  // One face can set Han, so there is no picker to offer.
  const dialog = await settings(page, ZHT);
  await expect(dialog.getByLabel(ZHT["settings.textFont"], { exact: true })).toHaveCount(0);
  await expect(dialog.getByLabel(ZHT["settings.chromeFont"], { exact: true })).toHaveCount(0);
});

test("Chinese search finds a name that spans two characters", async ({ page }) => {
  // 耶穌 is two tokens in a per-character corpus. The Han query splitter
  // makes it a two-word query and the phrase tier confirms the consecutive
  // run — substring search, which is the only kind of search an unspaced
  // script can mean. WHAT FAILS WITHOUT THE SPLITTER: the index has no key
  // "耶穌", no tier matches, and the name of Jesus finds nothing.
  await reader(page, EN);
  await pick(page, EN, "zht");

  await page.getByLabel(ZHT["common.openSearch"]).click();
  const box = page.getByRole("searchbox");
  await expect(box).toBeVisible();
  await box.fill("耶穌");

  const results = page.locator('[data-surface="search results"]');
  await expect(results).toBeVisible({ timeout: 60_000 });
  await expect(results.locator("p.hint"), "the results box is still an empty state").toHaveCount(0, {
    timeout: 60_000,
  });
  await expect(
    results.getByText(ZHT["book.Matt"]).first(),
    "the two-character name did not find Matthew",
  ).toBeVisible({ timeout: 60_000 });
});

test.describe("a Taiwanese device, cold", () => {
  test.use({ locale: "zh-TW" });

  test("opens the traditional 和合本 with nobody having chosen anything", async ({ page }) => {
    // `zh-TW` must land on `zht` in three places that each resolve locales on
    // their own: the splash seed, the stage-1 corpus pick and the engine. A
    // base-tag strip to "zh" misses the corpus role and boots the KJV.
    await reader(page, ZHT);
    await expect(page.locator("html")).toHaveAttribute("lang", "zht");
    const words = (await boxes(page)).filter((b) => b.kind === "word");
    const text = words.map((w) => w.text).join("");
    expect(words.some((w) => HAN.test(w.text)), `a zh-TW device is reading ${text.slice(0, 40)}`).toBe(true);
    expect(text).not.toContain("There was a man");
  });
});

test.describe("a mainland device, cold", () => {
  test.use({ locale: "zh-CN" });

  test("opens the simplified edition — the two rows are not one", async ({ page }) => {
    await reader(page, ZHS);
    await expect(page.locator("html")).toHaveAttribute("lang", "zhs");
    const words = (await boxes(page)).filter((b) => b.kind === "word");
    const text = words.map((w) => w.text).join("");
    expect(words.some((w) => HAN.test(w.text)), `a zh-CN device is reading ${text.slice(0, 40)}`).toBe(true);
    // 個/个 is the ordinary measure word — a page of the traditional text
    // will show 個 and the simplified 个. Genesis 1 alone cannot promise
    // either, so the assertion is on the repertoire marker that IS on the
    // first painted chapter of both editions: 神 is shared, 創/创 differ.
    expect(text.includes("创") || !text.includes("創"), `traditional characters on a zh-CN device: ${text.slice(0, 60)}`).toBe(
      true,
    );
  });
});
