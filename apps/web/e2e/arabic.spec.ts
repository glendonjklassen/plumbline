import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

// Arabic, end to end: the Van Dyck read right to left.
//
// Every assertion here fails against the app as it was before Arabic landed,
// and each fails for a DIFFERENT reason — which is the point, because the four
// ways this feature can be broken look nothing alike on screen:
//
//   1. the chrome does not mirror       (dir stays ltr; menus on the wrong side)
//   2. the text does not mirror         (legible words in reversed order — the
//                                        one that is easiest to ship, because
//                                        every individual word renders fine)
//   3. punctuation lands on the wrong side of a word
//   4. search finds nothing             (the reader types what they can type,
//                                        the index holds what the Van Dyck
//                                        prints, and the two never meet)
//
// The strings are read from the catalogue rather than typed, so this file does
// not become a second place the Arabic copy lives.

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
  const est = page.getByRole("button", { name: lang["intro.pathEstablished"] });
  const canvas = page.locator(".pane canvas").first();
  await expect(est.or(canvas)).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: lang["intro.start"] }).click();
  }
  await expect(canvas).toBeVisible({ timeout: 90_000 });
}

/** Pick a language the way a reader does — see language.spec.ts for why this
 *  drives the real picker instead of writing the config. */
async function pick(page: Page, now: Record<string, string>, want: string): Promise<void> {
  await page.getByLabel(now["common.menu"]).click();
  await page.locator(".menu").getByRole("button", { name: now["shell.settings"] }).click();
  const dialog = page.locator('[data-surface="settings"]');
  await expect(dialog).toBeVisible();
  await page.evaluate(() => ((globalThis as any).__beforeSwitch = true));
  await dialog.getByLabel(now["settings.language"], { exact: true }).selectOption({ label: want });
  await page.waitForFunction(
    () => !(globalThis as any).__beforeSwitch && !!(globalThis as any).__plumbline,
    undefined,
    { timeout: 180_000 },
  );
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 180_000 });
}

/** The display list the last frame painted, straight off the paint probe.
 *
 *  POLLED, the way reader-perf.spec.ts does it, and not read once: a visible
 *  canvas means the pane is mounted, not that a frame has been painted into it,
 *  and `paintProbe.items` is a WeakRef that is only written when a NEW list
 *  reaches the painter. Read eagerly it returns [] on a machine running four
 *  browsers at once — which reads exactly like "this device opened the wrong
 *  Bible", the failure these tests exist to report. */
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
  await pick(page, EN, "العربية");

  // 1. THE DOCUMENT MIRRORS. `dir` is what flips every logical margin, the
  //    order of a flex row, and which side the menus open on.
  await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
  await expect(page.locator("html")).toHaveAttribute("lang", "ar");

  // The text really is the Van Dyck and not the KJV under an Arabic interface.
  const list = await boxes(page);
  expect(list.length, "no display list on the paint probe").toBeGreaterThan(5);
  const words = list.filter((b) => b.kind === "word");
  expect(words.length).toBeGreaterThan(3);
  expect(words.some((w) => /[\u0600-\u06FF]/.test(w.text)), "no Arabic in the reader").toBe(true);

  // 2. THE TEXT MIRRORS. Take the first line that holds several words and check
  //    they descend from right to left — i.e. that the engine's mirror ran.
  //    Without it every one of these words still renders perfectly, in exactly
  //    the wrong order, which is why this is asserted on coordinates rather
  //    than left to a screenshot.
  const firstY = Math.min(...words.map((w) => w.y));
  const line = words.filter((w) => w.y === firstY).sort((a, b) => list.indexOf(a) - list.indexOf(b));
  expect(line.length, "the first line has too few words to order").toBeGreaterThan(2);
  for (let i = 1; i < line.length; i++) {
    expect(
      line[i].x,
      `word ${i} ("${line[i].text}") is not left of word ${i - 1} ("${line[i - 1].text}") — the line reads left to right`,
    ).toBeLessThan(line[i - 1].x);
  }

  // 3. THE VERSE NUMBER LEADS AT THE RIGHT EDGE, where an Arabic reader's eye
  //    starts, and outboard of the verse's first word.
  const num = list.find((b) => b.kind === "verseNumber" && b.y === firstY);
  expect(num, "no verse number on the first line").toBeTruthy();
  expect(num!.x).toBeGreaterThan(line[0].x);
  const rightmost = Math.max(...list.filter((b) => b.y === firstY).map((b) => b.x + b.w));
  expect(num!.x + num!.w).toBeCloseTo(rightmost, 0);
});

test.describe("an Arabic device, cold", () => {
  test.use({ locale: "ar-EG" });

  // THE WHOLE FEATURE, from the one angle the tests above cannot see: they all
  // reach Arabic through the language picker, which means they prove the app
  // can BE Arabic, not that it arrives that way.
  //
  // It did not. An Arabic phone opened this app in fluent Arabic and showed the
  // reader the English KJV — the Van Dyck was an `optional` download gated on
  // whether the reader had installed it, and on a first visit nobody has
  // installed anything. Nothing errored. The chrome was correct. The app simply
  // told someone, in their own language, that its Bible was in another one.
  //
  // Asserted on the SCRIPTURE, because the chrome was already right when this
  // was broken. See the block in language.spec.ts for the German and Spanish
  // halves and the three separate regressions this shape catches.
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
    // Not "some Arabic is on screen": the chrome is Arabic too. This is the
    // canvas display list, which only the corpus feeds.
    expect(words.slice(0, 6).map((w) => w.text).join(" ")).not.toContain("There was a man");
  });
});

test("Arabic search finds a word the reader can actually type", async ({ page }) => {
  await reader(page, EN);
  await pick(page, EN, "العربية");

  // The Van Dyck prints "ٱلْبَدْءِ" — an alef wasla nobody has a key for, under
  // full vowelling nobody types. A reader searches for "البدء".
  //
  // WHAT FAILS WITHOUT THE FOLD: nothing errors. The search runs, the index is
  // intact, and it returns zero results — because `char::is_alphanumeric` is
  // TRUE for every Arabic mark (they carry Other_Alphabetic), so the index kept
  // the whole vowelling and the query never matches it. A silent empty result
  // is why this is a test and not a spot-check.
  await page.getByLabel(AR["common.openSearch"]).click();
  const box = page.getByRole("searchbox");
  await expect(box).toBeVisible();
  await box.fill("البدء");

  const results = page.locator('[data-surface="search results"]');
  await expect(results).toBeVisible({ timeout: 60_000 });
  // NOT "some Arabic appears in the results box". Both of the empty states —
  // "searching…" and the hint — are Arabic prose in this locale, and 297 of the
  // catalogue's strings carry tashkeel, so neither a script test nor a
  // diacritic test can tell chrome from scripture here. Two assertions that
  // only a real hit can satisfy:
  await expect(results.locator("p.hint"), "the results box is still an empty state").toHaveCount(0, {
    timeout: 60_000,
  });
  // …and the hit is the verse the word is in, named in Arabic.
  await expect(
    results.getByText(AR["book.Gen"]).first(),
    "searching for the unvowelled spelling did not find Genesis 1:1",
  ).toBeVisible({ timeout: 60_000 });
});

test("the scripture font picker offers only a face that has Arabic in it", async ({ page }) => {
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
  expect(enFonts).toContain("EB Garamond");
  expect(enFonts, "a naskh face is offered to an English reader").not.toContain("Amiri");
  await page.keyboard.press("Escape");

  await pick(page, EN, "العربية");
  await page.getByLabel(AR["common.menu"]).click();
  await page.locator(".menu").getByRole("button", { name: AR["shell.settings"] }).click();
  dialog = page.locator('[data-surface="settings"]');
  await expect(dialog).toBeVisible();
  const arFonts = await dialog
    .getByLabel(AR["settings.textFont"], { exact: true })
    .locator("option")
    .allTextContents();
  // Offering the other five would offer five ways to read nothing: fallback
  // renders the scripture in Amiri regardless, so the only thing the choice
  // would change is the SIZE, via the selected token's optical scale.
  expect(arFonts).toEqual(["Amiri"]);
});
