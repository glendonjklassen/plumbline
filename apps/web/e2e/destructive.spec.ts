import { expect, test, type Page } from "@playwright/test";

// Nothing is destroyed without asking, and saying no destroys nothing.
//
// A CLASS test, for the same reason as e2e/surfaces.spec.ts: the app had four
// different answers to "does this ask first?" (2026-07-29). Deleting a memorize
// card asked nothing. Rejecting a suggested weave asked nothing. Untagging asked
// nothing. Deleting a thread asked, via a prompt built by hand at its own call
// site. Whether an action asks should be a property of the action, not of whoever
// wrote its button — so there is one mechanism (`session.askConfirm`) and this
// checks every caller of it.
//
// Each case asserts BOTH halves, because a confirmation that appears and then
// deletes anyway is worse than none:
//   1. the confirmation appears;
//   2. Cancel leaves the thing intact;
//   3. the destructive button actually destroys it.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

const confirm = (page: Page) => page.locator('[data-surface="confirm"]');

test("deleting a thread asks, and Cancel keeps it", async ({ page }) => {
  await boot(page);

  // A thread to delete. The stock set seeds "Romans Road", so this uses its own.
  await page.evaluate(() =>
    (window as any).__plumbline.author("threadAdd", "Doomed", "John 3:16", null, new Date().toISOString()),
  );
  const names = async () =>
    ((await page.evaluate(() => (window as any).__plumbline.rpc.call("threads"))) as any).threads.map(
      (t: any) => t.name,
    );
  expect(await names()).toContain("Doomed");

  await page.evaluate(() => ((window as any).__plumbline.threadPickFor = "John 3:16"));
  await page.locator(".row", { hasText: "Doomed" }).locator("button.del").click();

  await expect(confirm(page)).toBeVisible();
  await expect(confirm(page)).toContainText("Doomed");
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  await expect(confirm(page)).toBeHidden();
  expect(await names(), "Cancel must not delete").toContain("Doomed");

  // Now mean it.
  await page.locator(".row", { hasText: "Doomed" }).locator("button.del").click();
  await confirm(page).getByRole("button", { name: "Delete thread" }).click();
  await expect.poll(names, { timeout: 15_000 }).not.toContain("Doomed");
});

test("deleting a tag asks, and Cancel keeps it", async ({ page }) => {
  await boot(page);

  await page.evaluate(() =>
    (window as any).__plumbline.author("tagAdd", "DoomedTag", "verse", "John 3:16", null, new Date().toISOString()),
  );
  const names = async () =>
    ((await page.evaluate(() => (window as any).__plumbline.rpc.call("tags"))) as any).tags.map(
      (t: any) => t.name,
    );
  expect(await names()).toContain("DoomedTag");

  await page.evaluate(() => ((window as any).__plumbline.tagPickFor = "John 3:16"));
  await page.locator(".row", { hasText: "DoomedTag" }).locator("button.del").click();

  await expect(confirm(page)).toBeVisible();
  await expect(confirm(page)).toContainText("DoomedTag");
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  await expect(confirm(page)).toBeHidden();
  expect(await names(), "Cancel must not delete").toContain("DoomedTag");

  // Now mean it.
  await page.locator(".row", { hasText: "DoomedTag" }).locator("button.del").click();
  await confirm(page).getByRole("button", { name: "Delete tag" }).click();
  await expect.poll(names, { timeout: 15_000 }).not.toContain("DoomedTag");
});

test("deleting a weave from its compare card asks, and Cancel keeps it", async ({ page }) => {
  await boot(page);

  // A weave of this test's own — the stock set stays untouched.
  await page.evaluate(() =>
    (window as any).__plumbline.author("weaveAddLink", "DoomedWeave", "John 3:16", "Rom 5:8", new Date().toISOString()),
  );
  const weaves = async () =>
    ((await page.evaluate(() => (window as any).__plumbline.rpc.call("weaves"))) as any).weaves;
  const index = (await weaves()).find((w: any) => w.name === "DoomedWeave")?.index;
  expect(index).not.toBeUndefined();

  // Straight to the compare card — the delete link lives on its header line.
  await page.evaluate((i) => ((window as any).__plumbline.panel = { kind: "compare", index: i }), index);
  const del = page.getByRole("button", { name: "✕ delete weave" });
  await expect(del).toBeVisible({ timeout: 15_000 });

  await del.click();
  await expect(confirm(page)).toBeVisible();
  await expect(confirm(page)).toContainText("DoomedWeave");
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  await expect(confirm(page)).toBeHidden();
  expect(
    (await weaves()).map((w: any) => w.name),
    "Cancel must not delete",
  ).toContain("DoomedWeave");

  // Now mean it. The panel must also leave the dead card — ordinals shift.
  await del.click();
  await confirm(page).getByRole("button", { name: "Delete weave" }).click();
  await expect
    .poll(async () => (await weaves()).map((w: any) => w.name), { timeout: 15_000 })
    .not.toContain("DoomedWeave");
  expect(await page.evaluate(() => (window as any).__plumbline.panel?.kind)).toBe("weaves");
});

test("removing a memorize card asks, and Cancel keeps it", async ({ page }) => {
  await boot(page);

  await page.evaluate(() =>
    (window as any).__plumbline.author("memoryAdd", "John 3:16", new Date().toISOString()),
  );
  const cards = async () =>
    ((await page.evaluate(() =>
      (window as any).__plumbline.rpc.call("memoryCoverage", new Date().toISOString()),
    )) as any).cards.map((c: any) => c.ref);
  await expect.poll(cards, { timeout: 15_000 }).toContain("John 3:16");

  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.screen = "memorize";
    s.memorize = { view: "hub" };
  });
  const remove = page.locator("button.remove").first();
  await expect(remove).toBeVisible({ timeout: 15_000 });

  await remove.click();
  await expect(confirm(page), "removing a card must ask — it takes the review log with it").toBeVisible();
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  expect(await cards(), "Cancel must not remove the card").toContain("John 3:16");

  await page.locator("button.remove").first().click();
  await confirm(page).getByRole("button", { name: "Remove card" }).click();
  await expect.poll(cards, { timeout: 15_000 }).not.toContain("John 3:16");
});

test("clearing a chapter's reading history asks", async ({ page }) => {
  await boot(page);

  // A by-hand date is the one thing here that nothing else records, so losing it
  // to a stray tap is unrecoverable.
  await page.evaluate(() =>
    (window as any).__plumbline.author("readingMarkRead", "Gen", 1, "2026-01-01"),
  );
  const lastRead = async () =>
    ((await page.evaluate(() =>
      (window as any).__plumbline.rpc.call("readingChapters", "Gen", new Date().toISOString()),
    )) as any).chapters.find((c: any) => c.chapter === 1)?.lastRead ?? null;
  await expect.poll(lastRead, { timeout: 15_000 }).toBe("2026-01-01");

  await page.evaluate(() => ((window as any).__plumbline.markReadFor = { book: "Gen", chapter: 1 }));
  await page.getByRole("button", { name: "Clear history" }).click();
  await expect(confirm(page)).toBeVisible();
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  expect(await lastRead(), "Cancel must not clear it").toBe("2026-01-01");

  await page.getByRole("button", { name: "Clear history" }).click();
  await confirm(page).getByRole("button", { name: "Clear history" }).click();
  await expect.poll(lastRead, { timeout: 15_000 }).toBe(null);
});

// A note is the reader's own writing, and it used to be the only piece of it
// that could vanish with no affordance and no confirmation — "save an emptied
// editor" deletes the file (usernote.rs), and nothing asked. Both doors are
// covered here because BOTH are the action: the browser's ✕ and the emptied
// editor, same ask, same wording.
test("deleting a note from the browser asks, clears the gutter mark, and Cancel keeps it", async ({ page }) => {
  await boot(page);
  await page.evaluate(() =>
    (window as any).__plumbline.author("userNoteSet", "John 3:16", "Doomed note", new Date().toISOString()),
  );
  const notes = async () =>
    ((await page.evaluate(() => (window as any).__plumbline.rpc.call("userNotes"))) as any).notes.map(
      (n: any) => n.verse,
    );
  await expect.poll(notes, { timeout: 15_000 }).toContain("John 3:16");
  // The reader's gutter square is fed by this set; John 3 is the booted chapter.
  const marked = async () =>
    (await page.evaluate(() => Array.from((window as any).__plumbline.noteVerses("John", 3)))) as number[];
  await expect.poll(marked, { timeout: 15_000 }).toContain(16);

  await page.evaluate(() => ((window as any).__plumbline.panel = { kind: "notesBrowser" }));
  const del = page.locator(".nb-note .nb-del").first();
  await expect(del).toBeVisible({ timeout: 15_000 });

  await del.click();
  await expect(confirm(page)).toBeVisible();
  await expect(confirm(page)).toContainText("John 3:16");
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  await expect(confirm(page)).toBeHidden();
  expect(await notes(), "Cancel must not delete").toContain("John 3:16");

  // Now mean it.
  await del.click();
  await confirm(page).getByRole("button", { name: "Delete note" }).click();
  await expect.poll(notes, { timeout: 15_000 }).not.toContain("John 3:16");
  await expect.poll(marked, { timeout: 15_000 }).not.toContain(16);
});

// Mutation: in study/links.ts `editNote`, drop the empty-text askConfirm →
//   'Error: emptying the editor must ask before it deletes' (the confirm
//   surface never appears). A test that covered only the ✕ would PASS against
//   that mutation — and the emptied editor is the door readers actually use.
test("emptying the note editor asks too — it is the same delete", async ({ page }) => {
  await boot(page);
  await page.evaluate(() =>
    (window as any).__plumbline.author("userNoteSet", "John 3:16", "Doomed note", new Date().toISOString()),
  );
  const notes = async () =>
    ((await page.evaluate(() => (window as any).__plumbline.rpc.call("userNotes"))) as any).notes.map(
      (n: any) => n.verse,
    );
  await expect.poll(notes, { timeout: 15_000 }).toContain("John 3:16");
  const marked = async () =>
    (await page.evaluate(() => Array.from((window as any).__plumbline.noteVerses("John", 3)))) as number[];
  await expect.poll(marked, { timeout: 15_000 }).toContain(16);

  await page.evaluate(() => ((window as any).__plumbline.panel = { kind: "notesBrowser" }));
  const edit = page.locator(".nb-note .nb-edit").first();
  await expect(edit).toBeVisible({ timeout: 15_000 });

  // Clear the text and save — the delete spelled the way readers already do it.
  const field = page.locator('[role="dialog"] textarea');
  await edit.click();
  await expect(field).toBeVisible();
  await field.fill("");
  await page.getByRole("button", { name: "OK" }).click();

  await expect(confirm(page), "emptying the editor must ask before it deletes").toBeVisible();
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  await expect(confirm(page)).toBeHidden();
  expect(await notes(), "Cancel must not delete").toContain("John 3:16");

  // Again, and mean it.
  await edit.click();
  await expect(field).toBeVisible();
  await field.fill("");
  await page.getByRole("button", { name: "OK" }).click();
  await confirm(page).getByRole("button", { name: "Delete note" }).click();
  await expect.poll(notes, { timeout: 15_000 }).not.toContain("John 3:16");
  await expect.poll(marked, { timeout: 15_000 }).not.toContain(16);
});

test("a confirmation always settles — Escape and navigating away both mean no", async ({ page }) => {
  await boot(page);

  // `askConfirm` hands out a promise. If a dismissal path forgets to resolve it,
  // the caller waits forever and the action silently never happens — which looks
  // like a dead button rather than a bug, so it is worth pinning.
  const escaped = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const p = s.askConfirm("Escape?", "body", "Do it");
    await new Promise((r) => setTimeout(r, 50));
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    return Promise.race([p, new Promise((r) => setTimeout(() => r("HUNG"), 3000))]);
  });
  expect(escaped, "Escape resolves the promise as a no").toBe(false);

  const navigated = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const p = s.askConfirm("Navigate?", "body", "Do it");
    await new Promise((r) => setTimeout(r, 50));
    s.dismissTransient();
    return Promise.race([p, new Promise((r) => setTimeout(() => r("HUNG"), 3000))]);
  });
  expect(navigated, "navigating away resolves the promise as a no").toBe(false);
});
