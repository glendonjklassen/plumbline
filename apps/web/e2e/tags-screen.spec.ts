import { expect, test, type Page } from "@playwright/test";

// Tags became a PAGE under Study rather than a card that raised the library
// panel directly: there is more than one thing to do with a tag collection, and
// a card that can only do the first has nowhere to put the rest (maintainer,
// 2026-08-14). Browse is what it always did; Rename and Merge are the two
// operations a tag library actually accumulates a need for, because names drift
// ("grace", "Grace", "God's grace") and end up wanting to be one tag.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1100, height: 900 });
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** Two tags with one verse each, written through the engine the reader's own
 *  taps use. The stock set seeds tags too, so these are named distinctly.
 *
 *  AND THEN WAITS FOR THE SHELL TO SEE THEM, which is not ceremony — it is the
 *  whole reason this file was intermittently red on a loaded runner.
 *
 *  Two things sit between the write and the screen. `tagAdd` goes STRAIGHT to
 *  the engine, so the worker never posts the `authored` event the session
 *  invalidates its query cache on; and `q()` returns `null` on its first call
 *  and fills in later, so even a fresh cache is empty for a tick. A Tags page
 *  opened in that window derives `tags` as `[]` — the pick dialog lists nothing,
 *  the click for "Zeta mercy" times out, and the merge assertion fails against a
 *  merge that never ran. Which of those happened depended on machine load, which
 *  is why it passed here and failed in CI.
 */
async function seedTags(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.tagAdd("Zeta grace", "verse", "Eph 2:8", null, "2026-08-14T00:00:00Z");
    await s.engine.tagAdd("Zeta mercy", "verse", "Ps 23:6", null, "2026-08-14T00:00:00Z");
    s.invalidate();
  });
  await expect
    .poll(
      async () => {
        const names = await tagNames(page);
        return names.includes("Zeta grace") && names.includes("Zeta mercy");
      },
      { timeout: 15_000 },
    )
    .toBe(true);
}

async function openTags(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Study" }).first().click();
  await page.getByRole("button", { name: /^Tags/ }).click();
  await expect(page.locator(".bar h2")).toHaveText("Tags", { timeout: 15_000 });
}

const tagNames = (page: Page): Promise<string[]> =>
  page.evaluate(() => ((window as any).__plumbline.q("tags")?.tags ?? []).map((t: any) => t.name));

test("Tags is a page under Study, and ‹ returns to the hub", async ({ page }) => {
  await boot(page);
  await openTags(page);
  // A page, not the panel: the hub's cards are gone while it is up.
  await expect(page.getByRole("button", { name: /^Devotionals and reading plans/ })).toHaveCount(0);
  // Browsing IS the page: the tags render inline (the stock set seeds some),
  // with the organization actions as buttons after the list.
  await expect(page.locator(".tag-row").first()).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("button", { name: /^Rename a tag/ })).toBeVisible();

  // Tapping a tag opens its detail card directly — no Browse door between.
  await page.locator(".tag-row").first().click();
  await expect(page.locator(".panel")).toBeVisible();
  await page.keyboard.press("Escape");

  await page.locator(".bar .back").click();
  await expect(page.locator(".bar h2")).toHaveText("Study");
});

test("renaming keeps the tag's members", async ({ page }) => {
  await boot(page);
  await seedTags(page);
  await openTags(page);

  await page.getByRole("button", { name: /^Rename a tag/ }).click();
  await page.getByRole("dialog", { name: /Choose a tag/ }).getByRole("button", { name: "Zeta grace" }).click();
  const field = page.getByRole("dialog", { name: /New name/ }).locator("input");
  await field.fill("Zeta favour");
  await page.getByRole("button", { name: "OK" }).click();

  await expect
    .poll(async () => (await tagNames(page)).includes("Zeta favour"), { timeout: 15_000 })
    .toBe(true);
  const names = await tagNames(page);
  expect(names, "a rename is not a copy").not.toContain("Zeta grace");

  // The members came with it — the rename moved the file, it did not make a
  // new empty tag wearing the name.
  const members = await page.evaluate(() =>
    ((window as any).__plumbline.q("tags")?.tags ?? []).find((t: any) => t.name === "Zeta favour")?.members?.length,
  );
  expect(members).toBe(1);
});

test("merging asks first, names both sides, and deletes the source", async ({ page }) => {
  await boot(page);
  await seedTags(page);
  await openTags(page);

  await page.getByRole("button", { name: /^Merge two tags/ }).click();
  await page.getByRole("dialog", { name: /Choose a tag/ }).getByRole("button", { name: "Zeta mercy" }).click();
  // The tag being merged is not offered as its own destination.
  const into = page.getByRole("dialog", { name: /Choose the tag to merge/ });
  await expect(into.getByRole("button", { name: "Zeta mercy" })).toHaveCount(0);
  await into.getByRole("button", { name: "Zeta grace" }).click();

  // Destructive, so it asks — and the question names which one survives.
  const ask = page.getByRole("dialog", { name: /Merge “Zeta mercy” into “Zeta grace”/ });
  await expect(ask).toBeVisible();
  await ask.getByRole("button", { name: "Merge" }).click();

  await expect
    .poll(async () => (await tagNames(page)).includes("Zeta mercy"), { timeout: 15_000 })
    .toBe(false);
  const survivor = await page.evaluate(() =>
    ((window as any).__plumbline.q("tags")?.tags ?? []).find((t: any) => t.name === "Zeta grace")?.members?.length,
  );
  expect(survivor, "both verses ended up in the tag that was kept").toBe(2);
});

test("backing out of the merge question changes nothing", async ({ page }) => {
  await boot(page);
  await seedTags(page);
  await openTags(page);

  await page.getByRole("button", { name: /^Merge two tags/ }).click();
  await page.getByRole("dialog", { name: /Choose a tag/ }).getByRole("button", { name: "Zeta mercy" }).click();
  await page.getByRole("dialog", { name: /Choose the tag to merge/ }).getByRole("button", { name: "Zeta grace" }).click();
  await page.getByRole("button", { name: "Cancel" }).click();

  const names = await tagNames(page);
  expect(names).toContain("Zeta mercy");
  expect(names).toContain("Zeta grace");
});
