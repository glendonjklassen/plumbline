import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

// French, end to end. The cheapest language the registry has ever gained —
// Latin script, no new face, no direction question — which is exactly why its
// spec is short and aimed at the three things French alone exercises:
//
//   1. a cold French device opens the Ostervald, not the KJV — the corpus
//      role convention (`corpus:fr`) has to survive the locale resolution
//   2. the FONT PICKERS ARE PRESENT — the Indic spec asserts their absence
//      (one face per script); French is the first added language on the
//      script with five faces, and a filter written as "non-English hides
//      the picker" would pass every earlier spec
//   3. search finds the word INSIDE an elision — "l'homme" is indexed as
//      `homme`, which is the whole reason the tokenizer peels `l'` into pre
//
// The strings are read from the catalogues rather than typed, so this file
// does not become a second place the copy lives.

const cat = (code: string): Record<string, string> =>
  JSON.parse(readFileSync(new URL(`../../../crates/core/src/i18n/${code}.json`, import.meta.url), "utf8"));

const EN = cat("en");
const FR = cat("fr");

interface Box {
  x: number;
  y: number;
  w: number;
  text: string;
  kind: string;
}

async function reader(page: Page, lang: Record<string, string>): Promise<void> {
  await page.goto("/");
  const est = page.getByRole("button", { name: lang["intro.pathEstablished"] });
  const canvas = page.locator(".pane canvas").first();
  await expect(est.or(canvas)).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: lang["intro.start"] }).click();
  }
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

test("the picker offers the three new languages, endonym first", async ({ page }) => {
  // The three rows this change set added, spelled out the way the Indic spec
  // spells its five — a label rebuilt from the same lookups the code makes
  // would pass on any format.
  await reader(page, EN);
  const dialog = await settings(page, EN);
  const rows = await dialog.getByLabel(EN["settings.language"], { exact: true }).locator("option").allTextContents();
  for (const want of [
    "Français (French)",
    "中文（繁體） (Chinese (Traditional))",
    "中文（简体） (Chinese (Simplified))",
  ]) {
    expect(rows, `the English picker does not offer "${want}"`).toContain(want);
  }
});

test("fr: its own Bible, with the Latin font pickers still offered", async ({ page }) => {
  await reader(page, EN);
  await pick(page, EN, "fr");
  await expect(page.locator("html")).toHaveAttribute("lang", "fr");
  await expect(page.locator("html")).toHaveAttribute("dir", "ltr");

  // The text is the Ostervald, not the KJV under a French interface.
  const words = (await boxes(page)).filter((b) => b.kind === "word");
  expect(words.length).toBeGreaterThan(3);
  const line = words.map((w) => w.text).join(" ");
  expect(line, `a French reader is reading ${JSON.stringify(line.slice(0, 60))}`).not.toContain("There was a man");
  expect(/[àâçèéêëîïôùû]|qu'|l'|d'/.test(line), `no French in the reader: ${line.slice(0, 60)}`).toBe(true);

  // THE PICKERS ARE PRESENT — five Latin faces can set French, so hiding the
  // dropdown (right for Punjabi, one face) would be wrong here. This is the
  // converse of indic.spec.ts's toHaveCount(0).
  const dialog = await settings(page, FR);
  await expect(dialog.getByLabel(FR["settings.textFont"], { exact: true })).toHaveCount(1);
  await expect(dialog.getByLabel(FR["settings.chromeFont"], { exact: true })).toHaveCount(1);
});

test("French search finds the word inside an elision", async ({ page }) => {
  // "l'homme" is one whitespace token in the source; the tokenizer peels the
  // elision into `pre` so the index key is `homme` — the word a reader types.
  // WHAT FAILS WITHOUT THE PEEL: nothing errors, and a search for the second
  // most ordinary noun in the Bible returns only the verses that happen to
  // use it unelided.
  await reader(page, EN);
  await pick(page, EN, "fr");

  await page.getByLabel(FR["common.openSearch"]).click();
  const box = page.getByRole("searchbox");
  await expect(box).toBeVisible();
  await box.fill("homme");

  const results = page.locator('[data-surface="search results"]');
  await expect(results).toBeVisible({ timeout: 60_000 });
  await expect(results.locator("p.hint"), "the results box is still an empty state").toHaveCount(0, {
    timeout: 60_000,
  });
  await expect(
    results.getByText(FR["book.Gen"]).first(),
    "the elided word did not find Genesis",
  ).toBeVisible({ timeout: 60_000 });
});

test.describe("a French device, cold", () => {
  test.use({ locale: "fr-FR" });

  test("opens the Ostervald with nobody having chosen anything", async ({ page }) => {
    await reader(page, FR);
    await expect(page.locator("html")).toHaveAttribute("lang", "fr");
    await expect(page.locator("html")).toHaveAttribute("dir", "ltr");

    const words = (await boxes(page)).filter((b) => b.kind === "word");
    expect(words.length, "no words on the paint probe").toBeGreaterThan(5);
    const line = words.slice(0, 12).map((w) => w.text).join(" ");
    expect(line, `a French device is reading ${JSON.stringify(line)} — that is not its Bible`).not.toContain(
      "There was a man",
    );
    expect(/[àâçèéêëîïôùû]|qu'|l'|d'/.test(words.map((w) => w.text).join(" "))).toBe(true);
  });
});
