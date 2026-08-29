import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

// Punjabi and Hindi, end to end — and the reason they get a file of their own
// rather than a row in language.spec.ts.
//
// Every non-Latin question this app had ever answered, it had answered for
// Arabic, and Arabic answers them all the same way: it is the only script that
// is not Latin AND the only script that is not left to right, so "which faces
// can set this language" and "which way does the page run" had one answer and
// were written as one condition. Gurmukhi and Devanagari split them. Each test
// below fails against the app as it was, for a reason the others cannot see:
//
//   1. the page mirrors        — an LTR language routed through the RTL branch
//   2. the face is Garamond    — `readerFace` asked `isRtl()`, Punjabi is not,
//                                so the reader is handed a face with no
//                                Gurmukhi glyph in it and the Bible renders
//                                from whatever the system happens to have
//   3. the picker offers five  — same condition, other side
//   4. search finds nothing    — the nukta a reader cannot reliably type
//   5. a cold device reads the KJV in fluent Punjabi
//
// The strings are read from the catalogues rather than typed, so this file does
// not become a second place the copy lives.

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
  // By VALUE, not label — see the same helper in language.spec.ts.
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

    // 1. NOT MIRRORED. The converse of the Arabic test, and it is a real risk
    //    rather than a formality: the change that taught this app about scripts
    //    started from the one condition that conflated them, and the cheapest
    //    wrong generalisation is "not Latin, therefore mirror".
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

    // 2. THE FACE. The headline regression: `readerFace` used to ask `isRtl()`,
    //    which is FALSE here, so the reader was handed EB Garamond — a face
    //    with no Gurmukhi or Devanagari glyph in it. Nothing errors; the text
    //    renders through per-glyph fallback at a Latin face's optical scale, in
    //    whatever the browser found, measured in the worker against something
    //    else again. `bodyFont` is what the last frame really set on the canvas.
    await expect
      .poll(async () => page.evaluate(() => (globalThis as any).__plumblinePaint?.bodyFont ?? ""), {
        timeout: 30_000,
        message: "no frame painted after the switch",
      })
      .toContain(lang.face);

    // 3. NO FONT PICKER. One face can set this script, and a dropdown with one
    //    row is a control that cannot do anything.
    const dialog = await settings(page, lang.strings);
    await expect(dialog.getByLabel(lang.strings["settings.textFont"], { exact: true })).toHaveCount(0);
    await expect(dialog.getByLabel(lang.strings["settings.chromeFont"], { exact: true })).toHaveCount(0);
  });
}

test("the English picker names every language twice, so it can be handed over", async ({ page }) => {
  // THE APP IS BUILT TO BE HANDED OVER, and the endonym alone only serves the
  // person who already reads it. Someone offering their phone to a Hindi
  // speaker was looking at six scripts they cannot read with no way to tell
  // which row was the one.
  //
  // The rows ARE SPELLED OUT here, which is the one place in this file where
  // repeating the registry is the point — `each_variant_reaches_its_own_row` in
  // core::i18n makes the same trade. A test that rebuilt the label from the
  // same two fields the code reads would pass on any format, including the
  // endonym alone.
  await reader(page, EN);
  const dialog = await settings(page, EN);
  const options = await dialog
    .getByLabel(EN["settings.language"], { exact: true })
    .locator("option")
    .allTextContents();

  for (const want of [
    "German (Deutsch)",
    "Spanish (Español)",
    "Arabic (العربية)",
    "Punjabi (ਪੰਜਾਬੀ)",
    "Hindi (हिन्दी)",
  ]) {
    expect(options, `the picker does not offer "${want}"`).toContain(want);
  }
  // English is one word in both columns and gets no bracket: "English
  // (English)" is noise, and the dedupe is the only reason this is not a
  // straight template.
  expect(options).toContain("English");
  expect(options.join(" ")).not.toContain("English (English)");
});

test("a non-English picker has no English in it", async ({ page }) => {
  // The bracket is English-only, and that is a fact about the DATA rather than
  // a choice: `exonym` on the registry row is the language's English name, so
  // there is nothing to put in front of the bracket for a Hindi reader. They
  // get the endonyms alone, which is what every reader got before.
  //
  // This is the assertion that keeps the rule from being written as "always
  // bracket", which would put "Punjabi" in Latin script in front of a reader
  // who does not read it.
  await reader(page, EN);
  await pick(page, EN, "hi");
  const dialog = await settings(page, HI);
  const options = await dialog
    .getByLabel(HI["settings.language"], { exact: true })
    .locator("option")
    .allTextContents();
  expect(options).toContain("हिन्दी");
  expect(options).toContain("ਪੰਜਾਬੀ");
  expect(options.join(" "), "an English name leaked into a Hindi picker").not.toContain("Punjabi");
});

test("no painted word begins inside a grapheme cluster", async ({ page }) => {
  // The tokenizer splits on whitespace and peels punctuation off each end, and
  // in these scripts "punctuation" is a category test that a virama fails and a
  // matra passes. A peel that took one codepoint too many leaves a word opening
  // on a combining mark: it reassembles perfectly, the verse text is intact,
  // and the reader sees a dotted circle where a letter should be.
  //
  // `check-indic.py` asserts this over the built corpus; this asserts it over
  // what actually reached the painter, which is the other end of the same
  // pipeline — layout and the wire layer are between them.
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

  // ਫ਼ਿਲਿਪੁੱਸ — Philip, and the ਫ਼ is ਫ plus a nukta. Layouts differ on whether
  // that dot is reachable at all, so a reader types ਫਿਲਿਪੁੱਸ.
  //
  // WHAT FAILS WITHOUT THE FOLD: nothing errors, the index is intact, and the
  // search returns zero rows. What fails with the fold applied too GENEROUSLY —
  // every nukta dropped — is not visible here at all, which is why
  // `search::tests` holds that half: ਸ਼ would silently become ਸ through the
  // whole Bible.
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

  // The whole feature from the angle the tests above cannot see: they reach
  // Punjabi through the picker, which proves the app can BE Punjabi, not that
  // it arrives that way. Arabic shipped once as a fluent Arabic interface over
  // the English KJV. Asserted on the SCRIPTURE, because the chrome is the half
  // that was already right when that happened.
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
