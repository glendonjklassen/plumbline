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

// Types, filters and pages, and the Study page's prayer column.
//
// Each step can fail against the defect it names, reasoned rather than mutated:
//   - "no Answered button on Thanks" reads the row the engine served: the button
//     is drawn from the wire's `kinds[].answerable`, so a type table that lets
//     thanks be answered puts the button back. The edit to Request then shows
//     the button, so its absence was the type and not a row that never has one.
//   - the order is asserted across the page break. The 21 requests are stamped
//     in 2020 and the thanks is stamped now, so newest-first puts the thanks at
//     the head of page 1 and "Request 1" at the foot of page 2; oldest-first, or
//     a pager that re-slices the same rows, fails one end or the other.
//   - each filter is checked by a count only it produces (1 thanks, 21 requests),
//     and the two together by the no-match line, which an OR of the filters
//     would never reach.
//   - the Study column is read before and after "Prayed", so it cannot pass on
//     a constant.
test("a prayer has a type, thanks is not answered, and the list filters and pages", async ({ page }) => {
  await boot(page);
  await openPrayer(page);
  const section = page.locator('section[aria-label="Prayer"]');
  const list = page.locator('[data-surface="prayer-list"] .item');

  await page.getByPlaceholder("New prayer…").fill("A good harvest");
  await page.locator('[data-surface="prayer-kind"]').selectOption("thanks");
  await page.getByRole("button", { name: "Add a prayer", exact: true }).click();
  await expect(list).toHaveCount(1, { timeout: 30_000 });
  await expect(list.first()).toHaveAttribute("data-kind", "thanks");
  await expect(list.first()).toContainText("Thanks ·");
  await expect(list.first().getByRole("button", { name: "Prayed", exact: true })).toBeVisible();
  await expect(list.first().getByRole("button", { name: "Answered", exact: true })).toHaveCount(0);

  // The Study page: a prayer list, and nothing prayed today.
  const nav = page.locator(".bottom-nav");
  await nav.getByRole("button", { name: "Study", exact: true }).click();
  const prayed = page.locator('[data-surface="pace-prayer"] .pace-fact');
  await page.locator('[data-surface="pace"] .chip', { hasText: "Today" }).click();
  await expect(prayed).toHaveText("Not prayed", { timeout: 30_000 });
  await nav.getByRole("button", { name: "Pray", exact: true }).click();
  await list.first().getByRole("button", { name: "Prayed", exact: true }).click();
  await expect(list.first()).toContainText("Prayed 1 day", { timeout: 30_000 });
  await nav.getByRole("button", { name: "Study", exact: true }).click();
  await expect(prayed).toHaveText("Prayed", { timeout: 30_000 });
  await page.locator('[data-surface="pace"] .chip', { hasText: "7 days" }).click();
  await expect(prayed).toHaveText("Prayed 1 of 7 days");
  await nav.getByRole("button", { name: "Pray", exact: true }).click();

  // Twenty-one older requests, straight through the engine.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    for (let i = 1; i <= 21; i++) {
      const err = await s.author("prayerAdd", `Request ${i}`, "request", `2020-03-01T12:${String(i).padStart(2, "0")}:00Z`);
      if (err) throw new Error(err);
    }
  });
  const pager = page.locator('[data-surface="prayer-pager"]');
  await expect(pager).toContainText("Page 1 of 2", { timeout: 30_000 });
  await expect(list).toHaveCount(20);
  await expect(list.nth(0).locator(".title")).toHaveText("A good harvest");
  await expect(list.nth(1).locator(".title")).toHaveText("Request 21");
  await expect(list.nth(1).getByRole("button", { name: "Answered", exact: true })).toBeVisible();
  await pager.getByRole("button", { name: "Next" }).click();
  await expect(pager).toContainText("Page 2 of 2");
  await expect(list).toHaveCount(2);
  await expect(list.nth(1).locator(".title")).toHaveText("Request 1");

  // By type: the one thanks, back on the first page, with nothing to page.
  const filters = page.locator('[data-surface="prayer-filters"]');
  await filters.locator("select").selectOption("thanks");
  await expect(list).toHaveCount(1);
  await expect(list.first().locator(".title")).toHaveText("A good harvest");
  await expect(pager).toHaveCount(0);
  // By type and day together: nothing is both thanks and from 2020.
  await filters.locator('input[type="date"]').nth(1).fill("2020-03-01");
  await expect(list).toHaveCount(0);
  await expect(section.locator(".empty")).toHaveText("No prayers match the filters.");
  // By day alone: the 21 requests.
  await filters.locator("select").selectOption("");
  await expect(pager).toContainText("Page 1 of 2");
  await expect(list.first().locator(".title")).toHaveText("Request 21");
  await filters.getByRole("button", { name: "Clear" }).click();
  await expect(list.first().locator(".title")).toHaveText("A good harvest");

  // Thanks becomes a request, and can then be answered.
  await list.first().getByRole("button", { name: "Edit", exact: true }).click();
  await list.first().locator("select").selectOption("request");
  await list.first().getByRole("button", { name: "Save" }).click();
  await expect(list.first()).toHaveAttribute("data-kind", "request", { timeout: 30_000 });
  await expect(list.first().getByRole("button", { name: "Answered", exact: true })).toBeVisible();

  // The picked type is defined under the field, with a prayer of that type.
  // Read for three types, so a line that never follows the picker fails.
  const picker = page.locator('[data-surface="prayer-kind"]');
  const info = page.locator('[data-surface="prayer-kind-info"]');
  await expect(picker.locator("option")).toHaveText([
    "Request",
    "Intercession",
    "Praise",
    "Thanks",
    "Lament",
    "Imprecation",
    "Confession",
  ]);
  await picker.selectOption("imprecation");
  await expect(info).toContainText("Asking God to judge the wicked.");
  await expect(info.getByRole("button", { name: "Psalms 69:24" })).toBeVisible();
  await picker.selectOption("praise");
  await expect(info.getByRole("button", { name: "Psalms 145:1–3" })).toBeVisible();
  await picker.selectOption("lament");
  await expect(info.getByRole("button", { name: "Psalms 13:1–2" })).toBeVisible();

  // Confession is verses to read and nothing to add: the field and ＋ go, the
  // three passages come with their text from the reader's Bible, and the list
  // is as long as it was. The texts are matched on words of the verses
  // themselves, so a block of references with no verses under them fails.
  const count = await list.count();
  const pickerAt = (await picker.boundingBox())!.x;
  await picker.selectOption("confession");
  // The picker leads the row, so it has not moved now the field is gone.
  expect((await picker.boundingBox())!.x).toBe(pickerAt);
  await expect(page.getByPlaceholder("New prayer…")).toBeHidden();
  await expect(page.getByRole("button", { name: "Add a prayer", exact: true })).toHaveCount(0);
  const passages = page.locator('[data-surface="prayer-confession"] .passage');
  await expect(passages).toHaveCount(3);
  await expect(passages.nth(0).locator(".ref")).toHaveText("Psalms 51:1–4");
  await expect(passages.nth(0)).toContainText("Have mercy upon me, O God", { timeout: 30_000 });
  await expect(passages.nth(1)).toContainText("As far as the east is from the west");
  await expect(passages.nth(2).locator(".ref")).toHaveText("Hebrews 8:12");
  await expect(passages.nth(2)).toContainText("will I remember no more");
  await expect(list).toHaveCount(count);
  await expect(info).toHaveCount(0);

  // The example opens in the reader.
  await picker.selectOption("request");
  await info.getByRole("button", { name: "1 Samuel 1:11" }).click();
  await expect(section).toHaveCount(0);
  await expect(page.locator(".pane canvas").first()).toBeVisible();
});
