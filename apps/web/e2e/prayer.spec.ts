import { expect, test, type Page } from "@playwright/test";

// Pray: a destination of its own. One prayer taken through everything a prayer can
// do — added, prayed for, answered with a comment, moved back, deleted — and the
// list surviving a reload.
//
// Each step can fail against the defect it names, reasoned rather than mutated:
//   - the reload is the persistence check. The list lives under `prayers/` in
//     the home; if that directory is missing from `USER_DIRS` (engine/home.ts)
//     the write lands in the in-memory home only, the screen looks right, and
//     the prayer is gone after the reload.
//   - "Prayed" is asserted through the row's own count and the button's state,
//     both of which come back from the engine's read, not from local state.
//   - the delete is checked in the home's deletion records, because a delete
//     without one looks identical on screen and only shows on a later restore.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

async function openPrayer(page: Page): Promise<void> {
  await page.locator(".bottom-nav").getByRole("button", { name: "Pray", exact: true }).click();
  await expect(page.locator('section[aria-label="Prayer"]')).toBeVisible();
}

test("a prayer is added, prayed for, answered, moved back and deleted", async ({ page }) => {
  await boot(page);
  // The daily reminder, in the reader's row with the plans and the hints: with
  // nothing on the list it asks for a first prayer, and a tap opens this screen.
  // `null` before the engine answers and "add" after, so this cannot pass on a
  // chip that was never read from the home.
  const reminder = page.locator(".plan-chip-row [data-prayer]");
  await expect(reminder).toHaveAttribute("data-prayer", "add", { timeout: 30_000 });
  await expect(reminder).toHaveText("Add a prayer");
  await reminder.click();
  await expect(page.locator('section[aria-label="Prayer"]')).toBeVisible();
  await expect(page.locator(".bottom-nav button.on")).toHaveText(/Pray/);

  const list = page.locator('[data-surface="prayer-list"] .item');
  await expect(page.locator('section[aria-label="Prayer"] .empty')).toHaveText("No prayers yet.", { timeout: 30_000 });

  // Add: the field empties and the prayer is on the list.
  const field = page.getByPlaceholder("New prayer…");
  await field.fill("Anna's surgery");
  await page.getByRole("button", { name: "Add a prayer", exact: true }).click();
  await expect(list).toHaveCount(1, { timeout: 30_000 });
  await expect(list.first().locator(".title")).toHaveText("Anna's surgery");
  await expect(field).toHaveValue("");

  // The reminder now counts what is left to pray for today…
  await page.locator(".bottom-nav").getByRole("button", { name: "Read", exact: true }).click();
  await expect(reminder).toHaveText("1 prayer", { timeout: 30_000 });
  await reminder.click();

  // Prayed: counted, and not offered again today.
  await list.first().getByRole("button", { name: "Prayed", exact: true }).click();
  await expect(list.first()).toContainText("Prayed 1 day", { timeout: 30_000 });
  await expect(list.first().getByRole("button", { name: "Prayed today" })).toBeDisabled();

  // …and is gone for the day once it has been.
  await page.locator(".bottom-nav").getByRole("button", { name: "Read", exact: true }).click();
  await expect(page.locator(".pane canvas").first()).toBeVisible();
  await expect(reminder).toHaveCount(0);

  // In the home, not browser storage: it is still there after a reload.
  await page.evaluate(() => (window as any).__plumbline.rpc.flush());
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await openPrayer(page);
  await expect(list).toHaveCount(1, { timeout: 30_000 });
  await expect(list.first()).toContainText("Prayed 1 day");

  // Answered, with a comment: off the list and onto the Answered view.
  await list.first().getByRole("button", { name: "Answered", exact: true }).click();
  const prompt = page.getByRole("dialog", { name: "Comment (optional)" });
  await prompt.locator("input").fill("Went well");
  await prompt.getByRole("button", { name: "OK" }).click();
  await expect(list).toHaveCount(0, { timeout: 30_000 });
  await page.locator(".views .chip", { hasText: "Answered" }).click();
  const answered = page.locator('[data-surface="prayer-answered"] .item');
  await expect(answered).toHaveCount(1);
  await expect(answered.first()).toContainText("Anna's surgery");
  await expect(answered.first().locator(".answer")).toHaveText("Went well");
  await expect(answered.first()).toContainText(/Answered .*\d{4}/);
  await expect(answered.first()).toContainText("Prayed 1 day");

  // An answered prayer is a record: no controls on it until Edit is on.
  await expect(answered.first().getByRole("button")).toHaveCount(0);
  await page.locator(".views .chip.edit").click();
  await expect(answered.first().getByRole("button", { name: "Delete prayer" })).toBeVisible();

  // Moved back: the answer is dropped and it is on the list again.
  await answered.first().getByRole("button", { name: "Move to prayer list" }).click();
  await expect(page.locator('section[aria-label="Prayer"] .empty')).toHaveText("No answered prayers yet.", { timeout: 30_000 });
  await page.locator(".views .chip", { hasText: "Prayer list" }).click();
  await expect(list).toHaveCount(1);

  // Deleted, after asking, with a deletion record behind it.
  const id = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const d = new Date();
    const day = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    return (await s.rpc.call("prayers", day)).praying[0].id as string;
  });
  await list.first().getByRole("button", { name: "Delete prayer" }).click();
  await page.locator('[data-surface="confirm"] button.danger').click();
  await expect(list).toHaveCount(0, { timeout: 30_000 });
  const files = await page.evaluate(async () => {
    const out: Record<string, string> = {};
    for (const [path, bytes] of await (window as any).__plumbline.rpc.exportUserData()) {
      out[path] = new TextDecoder().decode(bytes);
    }
    return out;
  });
  expect(Object.keys(files).filter((p) => p.startsWith("prayers/")), "the prayer's file is gone").toEqual([]);
  expect(files["deleted/tombstones.json"] ?? "", "a delete records a deletion").toContain(`prayer:${id}`);
});
