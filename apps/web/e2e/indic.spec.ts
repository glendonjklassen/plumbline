import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

// Punjabi and Hindi, end to end. Arabic is both the only non-Latin script and the only RTL
// one, so "which faces can set this language" and "which way does the page run" had been
// written as one condition; Gurmukhi and Devanagari split them. Each test below fails for a
// reason the others cannot see: the page mirrors (an LTR language routed through the RTL
// branch), the face is Garamond (`readerFace` asked `isRtl()`), the picker offers five faces,
// search misses the nukta, or a cold device reads the KJV in fluent Punjabi.
//
// Strings are read from the catalogues, so this file is not a second place the copy lives.

const cat = (code: string): Record<string, string> =>
  JSON.parse(readFileSync(new URL(`../../../crates/core/src/i18n/${code}.json`, import.meta.url), "utf8"));

const EN = cat("en");
const PA = cat("pa");
const HI = cat("hi");

const GURMUKHI = /[਀-੿]/;
const DEVANAGARI = /[ऀ-ॿ]/;

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
  // By value, not label — see the same helper in language.spec.ts.
  await dialog.getByLabel(now["settings.language"], { exact: true }).selectOption(want);
  await page.waitForFunction(() => !(globalThis as any).__beforeSwitch && !!(globalThis as any).__plumbline, undefined, {
    timeout: 180_000,
  });
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 180_000 });
}

/** The display list the last frame painted — polled, for arabic.spec.ts's
 *  reason: a visible canvas means the pane is mounted, not painted into. */
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

for (const lang of [
  { code: "pa", endonym: "ਪੰਜਾਬੀ", strings: PA, script: GURMUKHI, face: "Noto Serif Gurmukhi" },
  { code: "hi", endonym: "हिन्दी", strings: HI, script: DEVANAGARI, face: "Noto Serif Devanagari" },
]) {
  test(`${lang.code}: its own Bible, in a face that can set it, running left to right`, async ({ page }) => {
    await reader(page, EN);
    await pick(page, EN, lang.code);

    // 1. Not mirrored. The converse of the Arabic test: the cheapest wrong
    //    generalisation is "not Latin, therefore mirror".
    await expect(page.locator("html")).toHaveAttribute("dir", "ltr");
    await expect(page.locator("html")).toHaveAttribute("lang", lang.code);

    // The text is theirs, not the KJV under a translated interface.
    const words = (await boxes(page)).filter((b) => b.kind === "word");
    expect(words.length).toBeGreaterThan(3);
    expect(words.some((w) => lang.script.test(w.text)), `no ${lang.code} script in the reader`).toBe(true);

    // A line reads LEFT to right — the mirror did not run.
    const firstY = Math.min(...words.map((w) => w.y));
    const line = words.filter((w) => w.y === firstY);
    expect(line.length, "the first line has too few words to order").toBeGreaterThan(2);
    for (let i = 1; i < line.length; i++) {
      expect(line[i].x, `word ${i} is not right of word ${i - 1} — the line is mirrored`).toBeGreaterThan(
        line[i - 1].x,
      );
    }

    // 2. The face. `readerFace` used to ask `isRtl()`, false here, so the reader
    //    got EB Garamond — no Gurmukhi or Devanagari glyph in it. Nothing errors:
    //    the text renders through per-glyph fallback at a Latin face's optical
    //    scale, measured in the worker against something else again. `bodyFont`
    //    is what the last frame really set on the canvas.
    await expect
      .poll(async () => page.evaluate(() => (globalThis as any).__plumblinePaint?.bodyFont ?? ""), {
        timeout: 30_000,
        message: "no frame painted after the switch",
      })
      .toContain(lang.face);

    // 3. No font picker: one face can set this script, and a one-row dropdown is
    //    a control that cannot do anything.
    const dialog = await settings(page, lang.strings);
    await expect(dialog.getByLabel(lang.strings["settings.textFont"], { exact: true })).toHaveCount(0);
    await expect(dialog.getByLabel(lang.strings["settings.chromeFont"], { exact: true })).toHaveCount(0);
  });
}

test("every picker names each language twice, in the reader's own words", async ({ page }) => {
  // The endonym leads, because the row belongs to the person being handed the phone, and the
  // reader's own name for the language follows in brackets — the endonym alone leaves someone
  // offering their phone looking at six scripts they cannot read.
  //
  // The rows are spelled out on purpose: a test that rebuilt the label from the same two
  // lookups the code makes would pass on any format, including the endonym alone.
  await reader(page, EN);
  const options = async (lang: Record<string, string>) =>
    (await settings(page, lang))
      .getByLabel(lang["settings.language"], { exact: true })
      .locator("option")
      .allTextContents();

  const en = await options(EN);
  for (const want of [
    "Deutsch (German)",
    "Español (Spanish)",
    "العربية (Arabic)",
    "ਪੰਜਾਬੀ (Punjabi)",
    "हिन्दी (Hindi)",
  ]) {
    expect(en, `the English picker does not offer "${want}"`).toContain(want);
  }
  // The reader's own language takes no bracket. Not a special case for English: the same
  // comparison silences "Deutsch (Deutsch)" in German and "हिन्दी (हिन्दी)" in Hindi below.
  expect(en).toContain("English");
  expect(en.join(" ")).not.toContain("English (English)");
  await page.keyboard.press("Escape");

  // And it is not an English feature: the bracket is a catalogue lookup (`lang.<code>`), not
  // the registry's `exonym`, which is the English name.
  await pick(page, EN, "hi");
  const hi = await options(HI);
  for (const want of ["English (अंग्रेज़ी)", "Deutsch (जर्मन)", "ਪੰਜਾਬੀ (पंजाबी)", "العربية (अरबी)"]) {
    expect(hi, `the Hindi picker does not offer "${want}"`).toContain(want);
  }
  expect(hi).toContain("हिन्दी");
  expect(hi.join(" "), "the Hindi reader's own row is bracketed").not.toContain("हिन्दी (हिन्दी)");
  // The English name must not leak into a picker whose reader does not read it.
  expect(hi.join(" "), "an English name leaked into a Hindi picker").not.toContain("Punjabi");
});

test("no painted word begins inside a grapheme cluster", async ({ page }) => {
  // The tokenizer peels punctuation off each end, and in these scripts "punctuation" is a
  // category test a virama fails and a matra passes. One codepoint too many leaves a word
  // opening on a combining mark: the verse text reassembles perfectly and the reader sees a
  // dotted circle. `check-indic.py` asserts this over the built corpus; this asserts it over
  // what reached the painter, with layout and the wire layer in between.
  await reader(page, EN);
  await pick(page, EN, "hi");
  const words = (await boxes(page)).filter((b) => b.kind === "word");
  expect(words.length).toBeGreaterThan(3);
  const orphans = words.filter((w) => /^[ऀ-ःऺ-ॏ॑-ॗॢॣ]/.test(w.text));
  expect(orphans.map((o) => o.text), "a painted word starts with a combining mark").toEqual([]);
});

test("Punjabi search finds a word spelled without the dot a reader cannot type", async ({ page }) => {
  await reader(page, EN);
  await pick(page, EN, "pa");

  // ਫ਼ਿਲਿਪੁੱਸ — Philip; the ਫ਼ is ਫ plus a nukta that many layouts cannot reach, so a reader
  // types ਫਿਲਿਪੁੱਸ. Without the fold nothing errors and the search returns zero rows. The
  // opposite failure — the fold applied too generously, so ਸ਼ silently becomes ਸ through the
  // whole Bible — is not visible here, and lives in `search::tests`.
  await page.getByLabel(PA["common.openSearch"]).click();
  const box = page.getByRole("searchbox");
  await expect(box).toBeVisible();
  await box.fill("ਫਿਲਿਪੁੱਸ");

  const results = page.locator('[data-surface="search results"]');
  await expect(results).toBeVisible({ timeout: 60_000 });
  await expect(results.locator("p.hint"), "the results box is still an empty state").toHaveCount(0, {
    timeout: 60_000,
  });
  await expect(
    results.getByText(PA["book.Acts"]).first(),
    "the undotted spelling did not find Philip in Acts",
  ).toBeVisible({ timeout: 60_000 });
});

test.describe("a Punjabi device, cold", () => {
  test.use({ locale: "pa-IN" });

  // The angle the tests above cannot see: they reach Punjabi through the picker, which
  // proves the app can be Punjabi, not that it arrives that way. Arabic once shipped as a
  // fluent Arabic chrome over the English KJV, so this asserts on the scripture.
  test("opens its own Bible, in Punjabi, with nobody having chosen anything", async ({ page }) => {
    await reader(page, PA);
    await expect(page.locator("html")).toHaveAttribute("lang", "pa");
    await expect(page.locator("html")).toHaveAttribute("dir", "ltr");

    const words = (await boxes(page)).filter((b) => b.kind === "word");
    expect(words.length, "no words on the paint probe").toBeGreaterThan(5);
    expect(
      words.some((w) => GURMUKHI.test(w.text)),
      `a Punjabi device is reading ${JSON.stringify(words.slice(0, 8).map((w) => w.text))} — that is not its Bible`,
    ).toBe(true);
    expect(words.slice(0, 6).map((w) => w.text).join(" ")).not.toContain("There was a man");
  });
});
