import { expect, test, type Page } from "@playwright/test";

// TAG HOUSEKEEPING WHERE THE READER IS (feedback, 2026-10-11): a reader looked
// for "rename a tag" in the study panel and for "take this verse out of a tag"
// in the tag sheet, and found neither — both lived only on the Tags screen and
// in the verse's own tag list. The flows now exist on the surfaces a reader is
// actually looking at when the need arises, through ONE implementation each
// (`study/tagActions`), so what a rename asks and says cannot differ by door.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

const tagNames = async (page: Page): Promise<string[]> =>
  ((await page.evaluate(() => (window as any).__plumbline.rpc.call("tags"))) as any).tags.map((t: any) => t.name);

async function makeTag(page: Page, tag: string, ref: string): Promise<void> {
  await page.evaluate(
    async ([tag, ref]) => {
      const s = (window as any).__plumbline;
      await s.author("tagAdd", tag, "verse", ref, null, new Date().toISOString());
      await s.fetchQ("tags");
    },
    [tag, ref],
  );
}

const prompt = (page: Page) => page.locator('.dialog[role="dialog"]');
const sheet = (page: Page) => page.locator('[data-surface="tag picker"]');

// MUTATION this would catch: drop the ✎ button from TagPicker's row, or make
// `renameTag` author `tagAdd` instead of `tagRename` — the old name survives
// and the poll below never sees the new one. Reasoned, not run (CLAUDE.md).
test("a tag is renamed from the picker sheet, through the shared prompt", async ({ page }) => {
  await boot(page);
  await makeTag(page, "kingdom", "Matt 6:33");

  await page.evaluate(() => ((window as any).__plumbline.tagPickFor = "John 3:16"));
  await sheet(page).locator(".row", { hasText: "kingdom" }).getByRole("button", { name: "Rename this tag" }).click();

  // The prompt opens pre-filled with the current name — a rename edits, it does
  // not retype.
  await expect(prompt(page)).toBeVisible();
  await expect(prompt(page).locator("input[data-modal-focus]")).toHaveValue("kingdom");
  await prompt(page).locator("input[data-modal-focus]").fill("kingdom of God");
  await prompt(page).locator("button.primary").click();

  await expect.poll(() => tagNames(page), { timeout: 15_000 }).toContain("kingdom of God");
  expect(await tagNames(page)).not.toContain("kingdom");
  // The sheet is still up and now lists the new name: the picker is where the
  // reader was, and a rename is not a reason to leave it.
  await expect(sheet(page).locator("button.tag", { hasText: "kingdom of God" })).toBeVisible();
});

// MUTATION this would catch: remove the `renametag:` run from `tag_detail`
// (there is no button to click), or drop the re-aim in links.ts. The library
// sorts by name, so renaming "mercy" to "zz mercy" moves it past "zeal": the
// card left on the old ordinal would now be zeal's card, and the last two
// assertions are exactly what a stale ordinal gets wrong. Reasoned, not run.
test("a tag is renamed from its own card in the study panel, and the card follows it", async ({ page }) => {
  await boot(page);
  await makeTag(page, "kingdom", "Matt 6:33");
  await makeTag(page, "mercy", "Titus 3:5");
  await makeTag(page, "zeal", "Rom 12:11");

  // The library sorts by name: kingdom 0, mercy 1, zeal 2.
  const index = (await tagNames(page)).indexOf("mercy");
  expect(index).toBeGreaterThanOrEqual(0);
  await page.evaluate((i) => ((window as any).__plumbline.panel = { kind: "tag", index: i }), index);
  const panel = page.locator("aside.panel");
  const rename = panel.getByRole("button", { name: "✎ rename tag" });
  await expect(rename).toBeVisible({ timeout: 15_000 });

  await rename.click();
  await expect(prompt(page)).toBeVisible();
  await expect(prompt(page).locator("input[data-modal-focus]")).toHaveValue("mercy");
  // Sorts AFTER zeal, so the ordinal changes: 0 kingdom, 1 zeal, 2 "zz mercy".
  await prompt(page).locator("input[data-modal-focus]").fill("zz mercy");
  await prompt(page).locator("button.primary").click();

  await expect.poll(() => tagNames(page), { timeout: 15_000 }).toContain("zz mercy");
  // The card still shows the tag that was renamed, under its new name — not
  // whatever now sits at the old ordinal. A tag card lists only its own name
  // and members, so zeal's name on it can only mean the card was re-aimed wrong.
  await expect(panel).toContainText("zz mercy", { timeout: 15_000 });
  await expect(panel).toContainText("Titus 3:5");
  await expect(panel).not.toContainText("zeal");
});

const confirm = (page: Page) => page.locator('[data-surface="confirm"]');
const membersOf = async (page: Page, tag: string): Promise<string[]> => {
  const tags = ((await page.evaluate(() => (window as any).__plumbline.rpc.call("tags"))) as any).tags;
  return (tags.find((t: any) => t.name === tag)?.members ?? []).map((m: any) => m.verse);
};

// MUTATION this would catch: drop the `carried` toggle in TagPicker so a tap on
// a carried tag ADDS again — the sheet closes on the add and the confirmation
// below never appears; or confirm-then-remove regardless of the answer — the
// Cancel half fails. Both halves asserted, as destructive.spec.ts does.
// Reasoned, not run.
test("a verse is taken out of a tag from the picker sheet, after asking", async ({ page }) => {
  await boot(page);
  await makeTag(page, "kingdom", "John 3:16");
  await makeTag(page, "mercy", "Titus 3:5");

  await page.evaluate(() => ((window as any).__plumbline.tagPickFor = "John 3:16"));
  const kingdom = sheet(page).locator("button.tag", { hasText: "kingdom" });
  const mercy = sheet(page).locator("button.tag", { hasText: "mercy" });
  // The sheet says which tags THIS verse already carries.
  await expect(kingdom).toHaveAttribute("aria-pressed", "true");
  await expect(mercy).toHaveAttribute("aria-pressed", "false");

  await kingdom.click();
  await expect(confirm(page)).toBeVisible();
  await expect(confirm(page)).toContainText("kingdom");
  // Named the way a reader says it, not as the wire key.
  await expect(confirm(page)).toContainText("John 3:16");
  await confirm(page).getByRole("button", { name: "Cancel" }).click();
  await expect(confirm(page)).toBeHidden();
  expect(await membersOf(page, "kingdom"), "Cancel must not remove").toContain("John 3:16");
  await expect(sheet(page), "the sheet stays open either way").toBeVisible();

  // Now mean it.
  await kingdom.click();
  await confirm(page).getByRole("button", { name: "Remove" }).click();
  await expect.poll(() => membersOf(page, "kingdom"), { timeout: 15_000 }).not.toContain("John 3:16");
  // The tag itself survives — only the verse left it — and the sheet, still
  // open, shows it unchecked, so the next tap would add the verse back.
  expect(await tagNames(page)).toContain("kingdom");
  await expect(sheet(page)).toBeVisible();
  await expect(kingdom).toHaveAttribute("aria-pressed", "false");
});
