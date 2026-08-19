import { expect, test, type Page } from "@playwright/test";

// TAG CATEGORIES (maintainer UAT, 2026-08-18): "tags need categories otherwise
// it'll be soooo long." A category is assigned on the MANAGEMENT screen only —
// never mid-reading — and the tag lists (picker + library panel) group under
// category headings the moment any tag has one, staying dead flat until then.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

async function makeTags(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const mk = (tag: string, ref: string) => s.author("tagAdd", tag, "verse", ref, null, new Date().toISOString());
    await mk("kingdom", "Matt 6:33");
    await mk("mercy", "Titus 3:5");
    await mk("zeal", "Rom 12:11");
    await s.fetchQ("tags");
  });
}

test("a category set on the management screen groups the picker and the library", async ({ page }) => {
  await boot(page);
  await makeTags(page);

  // The management screen's card, through the real dialogs.
  await page.evaluate(() => ((window as any).__plumbline.screen = "tags"));
  await page.getByRole("button", { name: /File under categories/ }).click();
  await page.locator('[data-surface="pick"]').getByRole("button", { name: "kingdom", exact: true }).click();
  const dialog = page.locator('.dialog[role="dialog"]');
  await dialog.locator("input[data-modal-focus]").fill("Doctrine");
  await dialog.locator("button.primary").click();

  // The tag PICKER groups: "Doctrine" heads its tag, the rest sit under the
  // no-category heading, which only exists because a real heading now does.
  await page.evaluate(() => ((window as any).__plumbline.screen = "read"));
  await page.evaluate(() => ((window as any).__plumbline.tagPickFor = "John 3:16"));
  const sheet = page.locator('[data-surface="tag picker"]');
  await expect(sheet.locator(".ghead").first()).toHaveText("Doctrine");
  await expect(sheet.locator(".ghead").nth(1)).toHaveText("No category");
  // Order: the filed tag under its heading, before the unfiled ones.
  const names = await sheet.locator("button.tag").allInnerTexts();
  expect(names[0]).toContain("kingdom");
  await page.keyboard.press("Escape");

  // The LIBRARY panel (core-built blocks) groups the same way.
  await page.evaluate(() => ((window as any).__plumbline.panel = { kind: "tags" }));
  const panel = page.locator("aside.panel");
  await expect(panel).toContainText("Doctrine");
  await expect(panel).toContainText("No category");
  const text = await panel.innerText();
  expect(text.indexOf("Doctrine"), "the heading precedes its tag").toBeLessThan(text.indexOf("kingdom"));
  expect(text.indexOf("kingdom"), "filed before unfiled").toBeLessThan(text.indexOf("mercy"));

  // And with NO categories anywhere, the lists stay flat — no headings appear
  // for readers who never file anything.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("tagSetCategory", "kingdom", "");
    await s.fetchQ("tags");
  });
  await expect(panel).not.toContainText("No category");
  await page.evaluate(() => ((window as any).__plumbline.tagPickFor = "John 3:16"));
  await expect(sheet.locator("button.tag").first()).toBeVisible();
  expect(await sheet.locator(".ghead").count(), "flat again once no tag is filed").toBe(0);
});

// The PICKER IDIOM for filing (UAT, 2026-08-18): once a category exists it is
// a list you tap — retyping "Doctrine" for every tag filed under it is how a
// typo quietly forks a second heading. Freetext survives behind "New
// category…", and "No category" clears without an empty-string convention.
//
// MUTATION: in TagsScreen.categorize, skip the `existing.length > 0` branch.
// Red: the pick dialog below never appears (the flow goes straight to text).
test("an existing category is picked, not retyped — and freetext stays behind New", async ({ page }) => {
  await boot(page);
  await makeTags(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("tagSetCategory", "kingdom", "Doctrine");
    await s.fetchQ("tags");
  });

  await page.evaluate(() => ((window as any).__plumbline.screen = "tags"));
  await page.getByRole("button", { name: /File under categories/ }).click();
  await page.locator('[data-surface="pick"]').getByRole("button", { name: "mercy", exact: true }).click();

  // The category question is a PICK now: the existing heading, a door to a new
  // one, and a way to clear.
  const pick = page.locator('[data-surface="pick"]');
  await expect(pick.getByRole("button", { name: "Doctrine", exact: true })).toBeVisible();
  await expect(pick.getByRole("button", { name: "New category…" })).toBeVisible();
  await expect(pick.getByRole("button", { name: "No category", exact: true })).toBeVisible();
  await pick.getByRole("button", { name: "Doctrine", exact: true }).click();

  // Both tags now sit under the one heading — no fork, no second "Doctrine".
  await page.evaluate(() => ((window as any).__plumbline.panel = { kind: "tags" }));
  const panel = page.locator("aside.panel");
  await expect(panel).toContainText("Doctrine");
  const text = await panel.innerText();
  expect(text.indexOf("kingdom")).toBeGreaterThan(text.indexOf("Doctrine"));
  expect(text.indexOf("mercy")).toBeGreaterThan(text.indexOf("Doctrine"));
  expect(text.match(/Doctrine/g)?.length, "one heading, not a fork").toBe(1);

  // And "New category…" opens the text prompt — the ADD path the UAT asked for.
  await page.evaluate(() => ((window as any).__plumbline.screen = "tags"));
  await page.getByRole("button", { name: /File under categories/ }).click();
  await page.locator('[data-surface="pick"]').getByRole("button", { name: "zeal", exact: true }).click();
  await pick.getByRole("button", { name: "New category…" }).click();
  const dialog = page.locator('.dialog[role="dialog"]');
  await dialog.locator("input[data-modal-focus]").fill("Virtue");
  await dialog.locator("button.primary").click();
  await expect
    .poll(async () =>
      page.evaluate(async () => {
        const tags = (await (window as any).__plumbline.fetchQ("tags"))?.tags ?? [];
        return tags.find((x: any) => x.name === "zeal")?.category ?? "";
      }),
    )
    .toBe("Virtue");
});
