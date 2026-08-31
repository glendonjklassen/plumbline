import { expect, test, type Page } from "@playwright/test";

// Authoring through the buttons a reader presses. Every other authoring test reaches the
// engine directly (`tagAdd`, `threadAdd`, `weaveFromTag`), so a broken `onclick`, a picker
// that authors the wrong verse, a "Create weave" that never fires, or a sheet unreachable
// from the reader would leave the whole existing suite green. The assertions are what the
// reader sees at each step; the written files are read at the end only to confirm it.
//
// Two things are deliberately not asserted:
//
//   * refKey-shaped labels ("John 3:16"), which are being replaced with display names — the
//     refKeys here come off the session (`contextMenu.refKey`, the frozen wire form) and are
//     used to check the files, never matched on screen.
//   * the long-press route into the same menu. The default project is a desktop viewport with
//     a mouse, and both routes meet in ReaderPane's `openContextMenu`.

test.setTimeout(180_000);

/** A tag name no stock weave, tag or thread uses, so every locator below is unambiguous. */
const TAG = "Grafted branches";
const TAG_FILE = "tags/grafted-branches.json";
const WEAVE_FILE = "weaves/grafted-branches.json";

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/**
 * One authored file, parsed, out of the tree the backup zip is built from.
 *
 * Polled rather than read once: the click that writes it resolves the shell's promise before
 * the engine's write has necessarily crossed back over the worker boundary.
 */
async function authoredJson(page: Page, path: string): Promise<any> {
  const read = (): Promise<string | null> =>
    page.evaluate(async (want) => {
      const files = new Map<string, Uint8Array>(await (window as any).__plumbline.rpc.exportUserData());
      const bytes = files.get(want);
      return bytes ? new TextDecoder().decode(bytes) : null;
    }, path);
  await expect
    .poll(async () => (await read()) !== null, {
      timeout: 30_000,
      message: `${path} never reached the tree the backup is built from`,
    })
    .toBe(true);
  return JSON.parse((await read())!);
}

/** The verse context menu. Told apart from the header's ≡ dropdown — both are
 *  `.menu` — by an item only this one has. */
const verseMenu = (page: Page) => page.locator(".menu", { hasText: "Copy chapter" });

/**
 * Right-click the chapter, and answer with the refKey the menu opened on.
 *
 * Retried: `boot` returns as soon as the header names the chapter, and the display list
 * arrives from the engine worker after that — a right-click against an empty list hit-tests
 * nothing and no menu opens. A chapter change clears the list again.
 */
async function openVerseMenu(page: Page): Promise<string> {
  const canvas = page.locator(".pane canvas").first();
  const box = (await canvas.boundingBox())!;
  const menu = verseMenu(page);
  for (let i = 0; i < 40; i++) {
    if (await menu.isVisible().catch(() => false)) break;
    // 0.4 of the width is inside the text column at any desktop width (centred, capped at
    // 720px); 60px down is the chapter's first line or two.
    await canvas.click({ button: "right", position: { x: box.width * 0.4, y: 60 }, timeout: 10_000 });
    if (await menu.isVisible().catch(() => false)) break;
    await page.waitForTimeout(200);
  }
  await expect(menu, "a right-click on the chapter must open the verse menu").toBeVisible();
  const ref = await page.evaluate(() => (window as any).__plumbline.contextMenu?.refKey ?? null);
  expect(ref, "the menu opened with no verse under it").not.toBeNull();
  return ref as string;
}

test("a reader tags two verses from the verse menu and turns the tag into a weave", async ({ page }) => {
  await boot(page);
  const panel = page.locator("aside.panel");
  const picker = page.locator('[data-surface="tag picker"]');

  // ── verse one: the menu → Tag… → a tag that does not exist yet ──
  const refA = await openVerseMenu(page);
  await verseMenu(page).getByRole("button", { name: "Tag…" }).click();
  await expect(picker, "Tag… must open the picker sheet").toBeVisible();

  await picker.getByPlaceholder("New tag…").fill(TAG);
  await picker.getByRole("button", { name: "＋", exact: true }).click();
  await expect(picker, "the sheet closes itself once the tag is added").toBeHidden();
  // Matched on the name alone — the toast's ref label is being reworked.
  await expect(page.locator(".toast", { hasText: TAG })).toBeVisible({ timeout: 15_000 });

  // ── verse two: one chapter on, the same tag, picked off the list ──
  //
  // Forward, and checked: both right-clicks land on the same point, so a swallowed ‹› tap
  // would silently re-tag verse one, and the canon-order assertion at the end is only
  // meaningful while the second verse is the later of the two.
  const chapterOne = (await page.locator(".subtitle").textContent()) ?? "";
  await page.locator(".nav button[title='Next chapter']").first().click();
  await expect(page.locator(".subtitle"), "the reader moved on a chapter").not.toHaveText(chapterOne);
  const refB = await openVerseMenu(page);
  expect(refB, "the second verse has to be a different verse").not.toBe(refA);
  await verseMenu(page).getByRole("button", { name: "Tag…" }).click();
  await expect(picker).toBeVisible();

  // The tag the first add authored is now pickable and the sheet says it holds one verse —
  // step one proven off the screen, and the branch a freetext-only test never takes.
  const row = picker.getByRole("button", { name: TAG });
  await expect(row, "the tag just created must be pickable, not retypable").toBeVisible({ timeout: 30_000 });
  await expect(row.locator(".count"), "the sheet should count the one verse it has").toHaveText("1");
  await row.click();
  await expect(picker).toBeHidden();

  // ── Study ▸ Tags: two members, and the conversion on offer ──
  await page.locator("nav.browse").getByRole("button", { name: "Study" }).click();
  await page.locator(".ex-card", { hasText: /^Tags/ }).click();
  const tagRow = page.locator(".tag-row", { hasText: TAG });
  await expect(tagRow, "both verses are in the tag").toContainText("2 members", { timeout: 30_000 });
  await tagRow.click();

  // "⇔ make weave" is emitted only for a tag with ≥2 verse members (core
  // `panel::tag_detail`), so finding it here is the second add restated.
  const makeWeave = panel.getByRole("button", { name: "make weave" });
  await expect(makeWeave, "a two-verse tag offers the conversion").toBeVisible({ timeout: 30_000 });
  await makeWeave.click();

  const sheet = page.getByRole("dialog").filter({ hasText: "Make a weave" });
  await expect(sheet, "the sheet defaults to every member checked").toContainText("2 of 2 passages");
  await sheet.getByRole("button", { name: "Create weave" }).click();
  await expect(sheet).toBeHidden();
  await expect(page.locator(".toast", { hasText: "2 passages chained" })).toBeVisible({ timeout: 30_000 });

  // ── the weave is in the library the reader browses ──
  await page.locator("nav.browse").getByRole("button", { name: "Study" }).click();
  await page.locator(".ex-card", { hasText: /^Weaves/ }).click();
  await page.locator(".ex-card", { hasText: /^Browse weaves/ }).click();
  await expect(
    panel.locator("p", { hasText: TAG }),
    "two chained passages are one link, and it is in the weave library",
  ).toContainText("1 link", { timeout: 30_000 });

  // ── the files, only to confirm what the UI already claimed ──
  const tag = await authoredJson(page, TAG_FILE);
  expect(tag.name).toBe(TAG);
  expect(
    tag.members.map((m: any) => m.target.ref).sort(),
    "the two verses in the file are the two the menu was opened on",
  ).toEqual([refA, refB].sort());

  const weave = await authoredJson(page, WEAVE_FILE);
  expect(weave.format, "the frozen on-disk tag").toBe("overlay-weave-v2");
  expect(weave.name).toBe(TAG);
  expect(weave.links).toHaveLength(1);
  // Endpoints in canon order: `weave::add_chain` sorts by reading key, and the second
  // verse is one chapter forward of the first.
  expect([weave.links[0].a, weave.links[0].b], "the chain runs through the canon, not in click order").toEqual([
    refA,
    refB,
  ]);
});
