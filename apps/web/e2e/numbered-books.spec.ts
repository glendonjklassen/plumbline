import { expect, test, type Page } from "@playwright/test";

// A numbered book must be reachable from every surface that hands the shell a
// refKey.
//
// The refKey is the frozen compact form — OSIS book id, one space, chapter:verse
// ("1John 3:16") — and three places turned it into the core's `go:` verb by
// hand. Each shell-side hand-parse is a chance to disagree with the core, which
// splits a refKey on its LAST space (`VRef::parse_ref_key`, `panel::go_uri`) and
// documents that a `go:` book may contain spaces. A disagreement doesn't warn:
// the verb parses into a book nobody has, `navigate` takes it (it clamps the
// chapter, never the book), and the reader watches a tap do nothing.
//
// So this covers all three surfaces with a numbered book — the arrival link, the
// notes browser, the memorize hub — and asserts what the reader would see: the
// destination bar naming the book, and the pane actually pointing at the verse.
//
// Written 2026-07-29 for the audit item that said numbered books dead-clicked in
// these three places because `replace(" ", ":")` replaces only the first space.
// They did not, and this file is the proof: all three tests pass against that
// exact code. Every OSIS id the corpus ships is one word ("1John", "2Chr"), so
// there is no second space for it to miss — the display name ("1 John") has the
// space, and no refKey is ever built from that. The three sites now use the
// core's rule anyway, and these tests hold the behaviour the item cared about.
//
// Mutation-tested one site at a time (dropping the translation, `go:${refKey}`),
// and each test failed for its own site while the other two stayed green:
//   * App.svelte      → test 1, destination bar reads "1John 3 16" (book "1John 3")
//   * StudyPanel      → test 2, "2Chr 7 14"
//   * MemorizeHost    → test 3, "1Pet 5 7"
// The subtitle is what makes those legible: `bookName` falls back to the raw id,
// so a book the canon doesn't have shows up as itself instead of its name.

async function boot(page: Page, url = "/"): Promise<void> {
  await page.goto(url);
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Where the active pane is pointing. */
async function where(page: Page): Promise<{ book: string; chapter: number; verse: number | null }> {
  return await page.evaluate(() => {
    const s = (window as any).__plumbline;
    const p = s.panes[s.activePane];
    return { book: p.book, chapter: p.chapter, verse: p.targetVerse };
  });
}

// The receiving end of a shared QR, for a book whose id starts with a digit.
test("a shared link to a numbered book opens at its verse", async ({ page }) => {
  await boot(page, "/?at=1John+3%3A16");
  await expect(page.locator(".subtitle")).toHaveText("1 John 3", { timeout: 90_000 });
  await expect.poll(() => where(page), { timeout: 15_000 }).toEqual({ book: "1John", chapter: 3, verse: 16 });
});

test("a note on a numbered book opens its verse from the notes browser", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const err = await s.author("userNoteSet", "2Chr 7:14", "If my people shall humble themselves…", new Date().toISOString());
    if (err) throw new Error(`userNoteSet: ${err}`);
    s.panel = { kind: "notesBrowser" };
  });
  const ref = page.locator(".nb-ref");
  await expect(ref).toHaveText("2 Chronicles 7:14", { timeout: 90_000 });
  await ref.click();

  await expect(page.locator(".subtitle")).toHaveText("2 Chronicles 7", { timeout: 90_000 });
  await expect.poll(() => where(page), { timeout: 15_000 }).toEqual({ book: "2Chr", chapter: 7, verse: 14 });
});

test("a memorize card on a numbered book opens its verse", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const err = await s.author("memoryAdd", "1Pet 5:7", new Date().toISOString());
    if (err) throw new Error(`memoryAdd: ${err}`);
    s.screen = "memorize";
    s.memorize = { view: "hub" };
  });
  const ref = page.locator(".card .ref");
  await expect(ref).toHaveText("1Pet 5:7", { timeout: 90_000 });
  await ref.click();

  await expect(page.locator(".subtitle")).toHaveText("1 Peter 5", { timeout: 90_000 });
  await expect.poll(() => where(page), { timeout: 15_000 }).toEqual({ book: "1Pet", chapter: 5, verse: 7 });
});
