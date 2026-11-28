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

/** Filter · Edit · New, at the end of the view row. */
function tool(page: Page, name: "Filter" | "Edit" | "Add a prayer") {
  return page.locator('[data-surface="prayer-tools"]').getByRole("button", { name, exact: true });
}

/** A new prayer through the form the New button opens. */
async function addPrayer(page: Page, title: string, kind?: string): Promise<void> {
  await tool(page, "Add a prayer").click();
  const form = page.getByRole("dialog", { name: "New prayer" });
  if (kind) await form.locator('[data-surface="prayer-kind"]').selectOption(kind);
  await form.getByPlaceholder("New prayer…").fill(title);
  await form.getByRole("button", { name: "Save" }).click();
  await expect(form).toHaveCount(0);
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

  // The screen is the list: no field and no pickers on it. A new prayer is a
  // form behind the New button, and with no rows Filter and Edit have nothing
  // to act on. A screen that still drew the field inline fails the count.
  const section = page.locator('section[aria-label="Prayer"]');
  await expect(section.locator("input, select")).toHaveCount(0);
  await expect(tool(page, "Filter")).toBeDisabled();
  await expect(tool(page, "Edit")).toBeDisabled();

  // Add: the form closes and the prayer is on the list.
  await addPrayer(page, "Anna's surgery");
  await expect(list).toHaveCount(1, { timeout: 30_000 });
  await expect(list.first().locator(".title")).toHaveText("Anna's surgery");
  // A row at rest holds what is done to a prayer day to day and nothing that
  // changes it: two buttons, neither of them Edit or Delete.
  await expect(list.first().getByRole("button")).toHaveText(["Prayed", "Answered"]);

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
  await tool(page, "Edit").click();
  await expect(tool(page, "Edit")).toHaveAttribute("aria-pressed", "true");
  await expect(answered.first().getByRole("button", { name: "Delete prayer" })).toBeVisible();

  // Moved back: the answer is dropped and it is on the list again.
  await answered.first().getByRole("button", { name: "Move to prayer list" }).click();
  await expect(page.locator('section[aria-label="Prayer"] .empty')).toHaveText("No answered prayers yet.", { timeout: 30_000 });
  await page.locator(".views .chip", { hasText: "Prayer list" }).click();
  await expect(list).toHaveCount(1);
  // Edit was turned on in Answered; it is not found armed here.
  await expect(tool(page, "Edit")).toHaveAttribute("aria-pressed", "false");
  await expect(list.first().getByRole("button", { name: "Delete prayer" })).toHaveCount(0);
  await tool(page, "Edit").click();

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
//   - the order is asserted across the page breaks. The 21 requests are stamped
//     in 2020 and the thanks is stamped now, so newest-first puts the thanks at
//     the head of page 1 and "Request 1" at the foot of page 3; oldest-first, or
//     a pager that re-slices the same rows, fails one end or the other. Ten to a
//     page: 22 rows are three pages, and a page size left at 20 reads "of 2".
//   - the filters are set in a dialog and read on the list behind it after it
//     is closed, so filters that only applied while their dialog was open, or
//     were dropped on close, fail; the Filter button says they are on.
//   - Edit is checked both ways: off, a row has no Edit button to press; on, it
//     has that and loses Prayed, so a toggle that drew both fails.
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

  await addPrayer(page, "A good harvest", "thanks");
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
  await expect(pager).toContainText("Page 1 of 3", { timeout: 30_000 });
  await expect(list).toHaveCount(10);
  await expect(list.nth(0).locator(".title")).toHaveText("A good harvest");
  await expect(list.nth(1).locator(".title")).toHaveText("Request 21");
  await expect(list.nth(1).getByRole("button", { name: "Answered", exact: true })).toBeVisible();
  await pager.getByRole("button", { name: "Next" }).click();
  await expect(pager).toContainText("Page 2 of 3");
  await expect(list).toHaveCount(10);
  await expect(list.nth(0).locator(".title")).toHaveText("Request 12");
  await pager.getByRole("button", { name: "Next" }).click();
  await expect(pager).toContainText("Page 3 of 3");
  await expect(list).toHaveCount(2);
  await expect(list.nth(1).locator(".title")).toHaveText("Request 1");
  await expect(pager.getByRole("button", { name: "Next" })).toBeDisabled();

  // The filters are behind the Filter button, which says whether any is set.
  const filters = page.getByRole("dialog", { name: "Filter" });
  const done = filters.getByRole("button", { name: "Done" });
  await expect(filters).toHaveCount(0);
  await expect(tool(page, "Filter")).toHaveAttribute("data-filtering", "false");
  // By type: the one thanks, back on the first page, with nothing to page.
  await tool(page, "Filter").click();
  await filters.locator("select").selectOption("thanks");
  await done.click();
  await expect(filters).toHaveCount(0);
  await expect(tool(page, "Filter")).toHaveAttribute("data-filtering", "true");
  await expect(list).toHaveCount(1);
  await expect(list.first().locator(".title")).toHaveText("A good harvest");
  await expect(pager).toHaveCount(0);
  // By type and day together: nothing is both thanks and from 2020.
  await tool(page, "Filter").click();
  await expect(filters.locator("select")).toHaveValue("thanks");
  await filters.locator('input[type="date"]').nth(1).fill("2020-03-01");
  await done.click();
  await expect(list).toHaveCount(0);
  await expect(section.locator(".empty")).toHaveText("No prayers match the filters.");
  // By day alone: the 21 requests.
  await tool(page, "Filter").click();
  await filters.locator("select").selectOption("");
  await done.click();
  await expect(pager).toContainText("Page 1 of 3");
  await expect(list.first().locator(".title")).toHaveText("Request 21");
  await tool(page, "Filter").click();
  await filters.getByRole("button", { name: "Clear" }).click();
  await done.click();
  await expect(tool(page, "Filter")).toHaveAttribute("data-filtering", "false");
  await expect(list.first().locator(".title")).toHaveText("A good harvest");

  // Thanks becomes a request, and can then be answered. A row has no Edit
  // button until Edit is on; then it has that in place of Prayed.
  await expect(list.first().getByRole("button", { name: "Edit", exact: true })).toHaveCount(0);
  await tool(page, "Edit").click();
  await expect(list.first().getByRole("button", { name: "Prayed today" })).toHaveCount(0);
  await expect(list.first().getByRole("button", { name: "Delete prayer" })).toBeVisible();
  await list.first().getByRole("button", { name: "Edit", exact: true }).click();
  const editForm = page.getByRole("dialog", { name: "Edit prayer" });
  await expect(editForm.getByPlaceholder("New prayer…")).toHaveValue("A good harvest");
  await expect(editForm.locator('[data-surface="prayer-kind"]')).toHaveValue("thanks");
  // Confession is read, not kept: it is not a type a prayer can be changed to.
  await expect(editForm.locator('[data-surface="prayer-kind"] option')).toHaveCount(6);
  await editForm.locator('[data-surface="prayer-kind"]').selectOption("request");
  await editForm.getByRole("button", { name: "Save" }).click();
  await expect(list.first()).toHaveAttribute("data-kind", "request", { timeout: 30_000 });
  await expect(list.first().locator(".title")).toHaveText("A good harvest");
  await tool(page, "Edit").click();
  await expect(list.first().getByRole("button", { name: "Answered", exact: true })).toBeVisible();

  // The picked type is defined under the picker, with a prayer of that type.
  // Read for three types, so a line that never follows the picker fails.
  await tool(page, "Add a prayer").click();
  const form = page.getByRole("dialog", { name: "New prayer" });
  const picker = form.locator('[data-surface="prayer-kind"]');
  const info = form.locator('[data-surface="prayer-kind-info"]');
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

  // Confession is verses to read and nothing to add: the field and Save go,
  // the three passages come with their text from the reader's Bible, and the
  // list is as long as it was. The texts are matched on words of the verses
  // themselves, so a block of references with no verses under them fails.
  const count = await list.count();
  await picker.selectOption("confession");
  await expect(form.getByPlaceholder("New prayer…")).toHaveCount(0);
  await expect(form.getByRole("button", { name: "Save" })).toHaveCount(0);
  const passages = form.locator('[data-surface="prayer-confession"] .passage');
  await expect(passages).toHaveCount(3);
  await expect(passages.nth(0).locator(".ref")).toHaveText("Psalms 51:1–4");
  await expect(passages.nth(0)).toContainText("Have mercy upon me, O God", { timeout: 30_000 });
  await expect(passages.nth(1)).toContainText("As far as the east is from the west");
  await expect(passages.nth(2).locator(".ref")).toHaveText("Hebrews 8:12");
  await expect(passages.nth(2)).toContainText("will I remember no more");
  await expect(list).toHaveCount(count);
  await expect(info).toHaveCount(0);

  // The example opens in the reader, and the form does not follow it there.
  await picker.selectOption("request");
  await info.getByRole("button", { name: "1 Samuel 1:11" }).click();
  await expect(section).toHaveCount(0);
  await expect(form).toHaveCount(0);
  await expect(page.locator(".pane canvas").first()).toBeVisible();
});
