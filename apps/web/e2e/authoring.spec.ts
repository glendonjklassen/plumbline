import { expect, test, type Page } from "@playwright/test";

// AUTHORING, THROUGH THE BUTTONS A READER ACTUALLY PRESSES.
//
// Every other authoring test in this suite reaches the engine directly —
// `__plumbline.engine.tagAdd(…)`, `threadAdd(…)`, `weaveFromTag(…)` — which
// proves the engine and nothing else. The verse menu, the tag sheet and the
// tag→weave sheet are the largest block of shipped UI with no coverage at all:
// a broken `onclick`, a picker that authors the wrong verse, a "Create weave"
// that never fires, or a sheet that cannot be reached from the reader would all
// leave the whole existing suite green.
//
// So this walks the one flow the product is built around — accumulate a topic
// now, organise it later (crates/ffi `plumbline_engine_weave_from_tag`) — with
// nothing but clicks and typing:
//
//   right-click a verse → Tag… → name a tag that does not exist
//   → next chapter → right-click → Tag… → pick that same tag off the list
//   → Explore ▸ Tags → the tag → "⇔ make weave" → Create weave
//   → Explore ▸ Weaves → it is in the library.
//
// The assertions are what the READER sees at each step: the sheet's own member
// count, the panel's "2 members", the fact that "⇔ make weave" is on offer at
// all (core `panel::tag_detail` only emits it at ≥2 verse members, so its
// presence IS the second add restated), and the weave's "1 link" in the library.
// The written files are read at the very end, through the same `exportUserData`
// the backup zip is built from, ONLY to confirm what the UI had already claimed.
//
// Two things are deliberately not asserted:
//
//   * refKey-shaped labels ("John 3:16"). They are being replaced with display
//     names, so the refKeys here are read off the SESSION (`contextMenu.refKey`,
//     the frozen wire form) and used to check the files, never matched on screen.
//   * the long-press route into the same menu. The default project is a desktop
//     viewport with a mouse; right-click and long-press meet in
//     ReaderPane's `openContextMenu`, and only the mouse half is driven here.

test.setTimeout(180_000);

/** A tag name no stock weave, tag or thread uses, so nothing merges into it and
 *  every locator below is unambiguous. */
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
 * One authored file, parsed, out of the tree the backup zip is built from (the
 * same helper `e2e/stable-ids.spec.ts` uses).
 *
 * Polled rather than read once: the click that writes it resolves the shell's
 * promise before the engine's write has necessarily crossed back over the
 * worker boundary, and a bare read would be a race that fails one run in ten.
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
 * Retried, not assumed. `boot` returns as soon as the header names the chapter,
 * and the display list arrives from the engine worker after that; a right-click
 * against an empty list hit-tests nothing, `ReaderPane.verseAt` answers null,
 * and no menu opens. A chapter change clears the list again, so the second call
 * needs the same patience.
 *
 * The refKey comes off the session, not off the menu's own label: `refKey` is
 * the frozen wire form, while the label is becoming a display name.
 */
async function openVerseMenu(page: Page): Promise<string> {
  const canvas = page.locator(".pane canvas").first();
  const box = (await canvas.boundingBox())!;
  const menu = verseMenu(page);
  for (let i = 0; i < 40; i++) {
    if (await menu.isVisible().catch(() => false)) break;
    // 0.4 of the width is inside the text column at any desktop width (the
    // column is centred and capped at 720px); 60px down is the chapter's first
    // line or two. Anywhere on the canvas would do — `verseAt` falls back to
    // the nearest verse number — but a hit on a real word is the reader's case.
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
  // The reader is told it happened, by name. (The toast also carries the ref;
  // only the name is matched, because that label is being reworked.)
  await expect(page.locator(".toast", { hasText: TAG })).toBeVisible({ timeout: 15_000 });

  // ── verse two: one chapter on, the SAME tag, picked off the list ──
  //
  // A chapter FORWARD, and checked: both right-clicks land on the same point, so
  // a swallowed ‹› tap would silently re-tag verse one, and the last assertion
  // in this test (endpoints in canon order) is only meaningful while the second
  // verse is the later of the two.
  const chapterOne = (await page.locator(".subtitle").textContent()) ?? "";
  await page.locator(".nav button[title='Next chapter']").first().click();
  await expect(page.locator(".subtitle"), "the reader moved on a chapter").not.toHaveText(chapterOne);
  const refB = await openVerseMenu(page);
  expect(refB, "the second verse has to be a different verse").not.toBe(refA);
  await verseMenu(page).getByRole("button", { name: "Tag…" }).click();
  await expect(picker).toBeVisible();

  // The tag the first add authored is now something the reader can PICK, and the
  // sheet says it holds one verse. That is step one proven off the screen rather
  // than out of a file — and it is the branch a freetext-only test never takes.
  const row = picker.getByRole("button", { name: TAG });
  await expect(row, "the tag just created must be pickable, not retypable").toBeVisible({ timeout: 30_000 });
  await expect(row.locator(".count"), "the sheet should count the one verse it has").toHaveText("1");
  await row.click();
  await expect(picker).toBeHidden();

  // ── Study ▸ Tags ▸ Browse: two members, and the conversion on offer ──
  // Tags is a PAGE now (2026-08-14) rather than a card that raised the library
  // straight away, so the library is one tap further in — the same route a
  // reader takes.
  await page.locator("nav.browse").getByRole("button", { name: "Study" }).click();
  await page.locator(".ex-card", { hasText: /^Tags/ }).click();
  await page.getByRole("button", { name: /^Browse tags/ }).click();
  await expect(panel.locator("p", { hasText: TAG }), "both verses are in the tag").toContainText("2 members", {
    timeout: 30_000,
  });
  await panel.getByRole("button", { name: TAG }).click();

  // "⇔ make weave" is emitted only for a tag with ≥2 verse members (core
  // `panel::tag_detail`), so finding it here is the second add, restated.
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

  // ── the files, ONLY to confirm what the UI already claimed ──
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
  // Endpoints in canon order — `weave::add_chain` sorts by reading key, and the
  // second verse is one chapter FORWARD of the first, so a is A and b is B.
  expect([weave.links[0].a, weave.links[0].b], "the chain runs through the canon, not in click order").toEqual([
    refA,
    refB,
  ]);
});
