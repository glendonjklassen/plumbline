import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

// Arabic, end to end: the Van Dyck read right to left. Each test fails for a
// different reason — the chrome does not mirror, the text does not mirror
// (legible words in reversed order), punctuation lands on the wrong side of a
// word, or search finds nothing because the reader cannot type what the index
// holds.
//
// Strings are read from the catalogues, so this file is not a second place the
// Arabic copy lives.

const EN: Record<string, string> = JSON.parse(
  readFileSync(new URL("../../../crates/core/src/i18n/en.json", import.meta.url), "utf8"),
);
const AR: Record<string, string> = JSON.parse(
  readFileSync(new URL("../../../crates/core/src/i18n/ar.json", import.meta.url), "utf8"),
);

/** One box from the live display list — what the engine handed the painter. */
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

/** Pick a language through the real picker, not by writing config — see language.spec.ts. */
async function pick(page: Page, now: Record<string, string>, want: string): Promise<void> {
  await page.getByLabel(now["common.menu"]).click();
  await page.locator(".menu").getByRole("button", { name: now["shell.settings"] }).click();
  const dialog = page.locator('[data-surface="settings"]');
  await expect(dialog).toBeVisible();
  await page.evaluate(() => ((globalThis as any).__beforeSwitch = true));
  // By value, not label — see the same helper in language.spec.ts.
  await dialog.getByLabel(now["settings.language"], { exact: true }).selectOption(want);
  await page.waitForFunction(
    () => !(globalThis as any).__beforeSwitch && !!(globalThis as any).__plumbline,
    undefined,
    { timeout: 180_000 },
  );
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 180_000 });
}

/** The display list the last frame painted, off the paint probe. Polled, not read once: a
 *  visible canvas means the pane is mounted, not painted into, and `paintProbe.items` is a
 *  WeakRef written only when a new list reaches the painter — read eagerly it returns [],
 *  which looks exactly like "this device opened the wrong Bible". */
async function boxes(page: Page): Promise<Box[]> {
  const read = () =>
    page.evaluate(() => {
      const items = (globalThis as any).__plumblinePaint?.items?.deref() ?? [];
      return items.map((i: any) => ({ x: i.x, y: i.y, w: i.w, text: i.text, kind: i.kind }));
    });
  await expect
    .poll(async () => (await read()).length, {
      timeout: 60_000,
      message: "no display list ever reached the painter",
    })
    .toBeGreaterThan(5);
  return await read();
}

test("the Arabic reader runs right to left, in the chrome and in the text", async ({ page }) => {
  await reader(page, EN);
  await pick(page, EN, "ar");

  // 1. The document mirrors: `dir` flips every logical margin, the order of a
  //    flex row, and which side the menus open on.
  await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
  await expect(page.locator("html")).toHaveAttribute("lang", "ar");

  // The text really is the Van Dyck and not the KJV under an Arabic interface.
  const list = await boxes(page);
  expect(list.length, "no display list on the paint probe").toBeGreaterThan(5);
  const words = list.filter((b) => b.kind === "word");
  expect(words.length).toBeGreaterThan(3);
  expect(words.some((w) => /[\u0600-\u06FF]/.test(w.text)), "no Arabic in the reader").toBe(true);

  // 2. The text mirrors: the first multi-word line descends right to left.
  //    Without the mirror every word still renders perfectly, in the wrong
  //    order, so this is asserted on coordinates rather than by screenshot.
  const firstY = Math.min(...words.map((w) => w.y));
  const line = words.filter((w) => w.y === firstY).sort((a, b) => list.indexOf(a) - list.indexOf(b));
  expect(line.length, "the first line has too few words to order").toBeGreaterThan(2);
  for (let i = 1; i < line.length; i++) {
    expect(
      line[i].x,
      `word ${i} ("${line[i].text}") is not left of word ${i - 1} ("${line[i - 1].text}") — the line reads left to right`,
    ).toBeLessThan(line[i - 1].x);
  }

  // 3. The verse number leads at the right edge, where an Arabic reader's eye
  //    starts, and outboard of the verse's first word.
  const num = list.find((b) => b.kind === "verseNumber" && b.y === firstY);
  expect(num, "no verse number on the first line").toBeTruthy();
  expect(num!.x).toBeGreaterThan(line[0].x);
  const rightmost = Math.max(...list.filter((b) => b.y === firstY).map((b) => b.x + b.w));
  expect(num!.x + num!.w).toBeCloseTo(rightmost, 0);
});

test.describe("an Arabic device, cold", () => {
  test.use({ locale: "ar-EG" });

  // The angle the tests above cannot see: they reach Arabic through the picker,
  // proving the app can be Arabic, not that it arrives that way. It once did
  // not — the Van Dyck was an `optional` download nobody has on a first visit,
  // so an Arabic phone got a fluent Arabic chrome over the English KJV, with
  // nothing errored. Asserted on the scripture, because the chrome was already
  // right. language.spec.ts holds the German and Spanish halves.
  test("opens the Van Dyck, in Arabic, with nobody having chosen anything", async ({ page }) => {
    await reader(page, AR);

    await expect(page.locator("html")).toHaveAttribute("lang", "ar");
    await expect(page.locator("html")).toHaveAttribute("dir", "rtl");

    const words = (await boxes(page)).filter((b) => b.kind === "word");
    expect(words.length, "no words on the paint probe").toBeGreaterThan(5);
    expect(
      words.some((w) => /[\u0600-\u06FF]/.test(w.text)),
      `an Arabic device is reading ${JSON.stringify(words.slice(0, 8).map((w) => w.text))} — that is not its Bible`,
    ).toBe(true);
    // Not "some Arabic is on screen" — the chrome is Arabic too. This is the
    // canvas display list, which only the corpus feeds.
    expect(words.slice(0, 6).map((w) => w.text).join(" ")).not.toContain("There was a man");
  });
});

test("Arabic search finds a word the reader can actually type", async ({ page }) => {
  await reader(page, EN);
  await pick(page, EN, "ar");

  // The Van Dyck prints "ٱلْبَدْءِ" — an alef wasla nobody has a key for, under full
  // vowelling nobody types; a reader searches for "البدء". Without the fold nothing
  // errors and the search returns zero rows: `char::is_alphanumeric` is true for every
  // Arabic mark (they carry Other_Alphabetic), so the index keeps the whole vowelling
  // and the query never matches it.
  await page.getByLabel(AR["common.openSearch"]).click();
  const box = page.getByRole("searchbox");
  await expect(box).toBeVisible();
  await box.fill("البدء");

  const results = page.locator('[data-surface="search results"]');
  await expect(results).toBeVisible({ timeout: 60_000 });
  // Not "some Arabic appears in the results box": both empty states are Arabic prose in
  // this locale and 297 catalogue strings carry tashkeel, so neither a script nor a
  // diacritic test can tell chrome from scripture. Two assertions only a real hit satisfies:
  await expect(results.locator("p.hint"), "the results box is still an empty state").toHaveCount(0, {
    timeout: 60_000,
  });
  // …and the hit is the verse the word is in, named in Arabic.
  await expect(
    results.getByText(AR["book.Gen"]).first(),
    "searching for the unvowelled spelling did not find Genesis 1:1",
  ).toBeVisible({ timeout: 60_000 });
});

test("a language one face can render gets that face, not a font menu", async ({ page }) => {
  await reader(page, EN);

  // English first: the five Latin faces, and NOT the naskh one.
  await page.getByLabel(EN["common.menu"]).click();
  await page.locator(".menu").getByRole("button", { name: EN["shell.settings"] }).click();
  let dialog = page.locator('[data-surface="settings"]');
  await expect(dialog).toBeVisible();
  const enFonts = await dialog
    .getByLabel(EN["settings.textFont"], { exact: true })
    .locator("option")
    .allTextContents();
  expect(enFonts.length, "an English reader has a real choice to make").toBeGreaterThan(1);
  expect(enFonts).toContain("EB Garamond");
  expect(enFonts, "a naskh face is offered to an English reader").not.toContain("Amiri");
  await page.keyboard.press("Escape");

  await pick(page, EN, "ar");
  await page.getByLabel(AR["common.menu"]).click();
  await page.locator(".menu").getByRole("button", { name: AR["shell.settings"] }).click();
  dialog = page.locator('[data-surface="settings"]');
  await expect(dialog).toBeVisible();
  // No pickers at all: only one face can render Arabic, and a one-row dropdown reads as
  // broken rather than restrained. Scripture face and chrome face are equally choiceless.
  await expect(dialog.getByLabel(AR["settings.textFont"], { exact: true })).toHaveCount(0);
  await expect(dialog.getByLabel(AR["settings.chromeFont"], { exact: true })).toHaveCount(0);
  await page.keyboard.press("Escape");

  // The face painting the scripture must be Amiri, resolved by `readerFace` rather than
  // left to CSS fallback, which renders Amiri glyphs at the Latin face's optical scale.
  // `bodyFont` is what the last frame really set on the canvas.
  await expect
    .poll(async () => page.evaluate(() => (globalThis as any).__plumblinePaint?.bodyFont ?? ""), {
      timeout: 30_000,
      message: "no frame painted after the switch",
    })
    .toContain("Amiri");

  // The round trip: back in English the pickers return with a non-blank selection. A
  // config holding an off-list token would otherwise leave the select showing nothing;
  // `readerFace` resolves such tokens to the language default, and the select binds to
  // the resolved face.
  await pick(page, AR, "en");
  await page.getByLabel(EN["common.menu"]).click();
  await page.locator(".menu").getByRole("button", { name: EN["shell.settings"] }).click();
  dialog = page.locator('[data-surface="settings"]');
  await expect(dialog).toBeVisible();
  const selected = await dialog
    .getByLabel(EN["settings.textFont"], { exact: true })
    .locator("option:checked")
    .allTextContents();
  expect(selected.length, "the English font select shows a BLANK selection").toBe(1);
  expect(selected[0].trim().length).toBeGreaterThan(0);
});
