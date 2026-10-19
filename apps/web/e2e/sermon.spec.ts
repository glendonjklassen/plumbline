import { expect, test, type Page } from "@playwright/test";

// The sermon editor: a thread written top to bottom. A new sermon is born with
// the five seeded headings; points and passages go under them and move by
// buttons; the order is the thread's outline, so it survives a reload and the
// entries follow it.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** The outline as the engine holds it: kinds, with a passage's verse. */
async function outline(page: Page, name: string): Promise<string[]> {
  return await page.evaluate(async (n) => {
    const s = (window as any).__plumbline;
    const t = ((await s.fetchQ("threads"))?.threads ?? []).find((x: any) => x.name === n);
    return (t?.outline ?? []).map((b: any) =>
      b.kind === "passage" ? `passage:${t.entries[b.entry].verse}` : `${b.kind}:${b.text}`,
    );
  }, name);
}

// Reasoned, not mutated: with the outline dropped from the write path every
// `outline` read below is empty and the first expectation fails; with the move
// not reaching the engine the reload step restores the pre-move order.
test("a sermon is shaped, ordered by buttons, and keeps its order across a reload", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => (window as any).__plumbline.openSermons("preach"));
  await expect(page.locator('[data-surface="sermons"]')).toBeVisible();

  // New sermon → named through the shared prompt → five seeded headings.
  await page.getByRole("button", { name: "New sermon" }).click();
  const prompt = page.locator('[aria-label="Sermon title"]');
  await expect(prompt).toBeVisible();
  await prompt.locator("input, textarea").first().fill("Grace alone");
  await prompt.locator("input, textarea").first().press("Enter");
  const editor = page.locator('[data-surface="sermon editor"]');
  await expect(editor).toBeVisible();
  await expect(editor.locator(".block.heading")).toHaveCount(5);
  await expect(editor.locator(".block.heading .text").first()).toHaveValue("Me");
  expect(await outline(page, "Grace alone")).toEqual(["heading:Me", "heading:Need", "heading:Bible", "heading:You", "heading:We"]);

  // A point under the first heading, typed and saved after the pause.
  await editor.getByRole("button", { name: "Add point" }).first().click();
  const point = editor.locator(".block.point .text").first();
  await expect(point).toBeFocused();
  await point.fill("Why this matters to me");
  await expect.poll(() => outline(page, "Grace alone"), { timeout: 10_000 }).toContain("point:Why this matters to me");

  // A passage under the same heading, resolved from what was typed.
  await editor.getByRole("button", { name: "Add passage" }).first().click();
  const ask = page.locator('[aria-label="Passage, for example John 3:16 or Ephesians 2:8-10"]');
  await expect(ask).toBeVisible();
  await ask.locator("input, textarea").first().fill("Ephesians 2:8-9");
  await ask.locator("input, textarea").first().press("Enter");
  await expect(editor.locator(".block.passage .ref")).toHaveText("Ephesians 2:8–9");
  await expect.poll(() => outline(page, "Grace alone")).toEqual([
    "heading:Me",
    "point:Why this matters to me",
    "passage:Eph 2:8",
    "heading:Need",
    "heading:Bible",
    "heading:You",
    "heading:We",
  ]);

  // Up one: the passage now precedes the point.
  await editor.locator(".block.passage").getByRole("button", { name: "Move up" }).click();
  await expect.poll(() => outline(page, "Grace alone")).toEqual([
    "heading:Me",
    "passage:Eph 2:8",
    "point:Why this matters to me",
    "heading:Need",
    "heading:Bible",
    "heading:You",
    "heading:We",
  ]);

  // A reload reads the same shape back from disk, and the entry carries its range.
  await page.reload();
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  expect((await outline(page, "Grace alone"))[1]).toBe("passage:Eph 2:8");
  const end = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const t = ((await s.fetchQ("threads"))?.threads ?? []).find((x: any) => x.name === "Grace alone");
    return t.entries[0].end;
  });
  expect(end).toBe(9);

  // The study panel reads the sermon in its preached order.
  await page.evaluate(() => (window as any).__plumbline.openSermons("explore", "Grace alone"));
  await expect(page.locator('[data-surface="sermon editor"] .block')).toHaveCount(7);
});

// A reference nothing in the text answers to is said so, and adds nothing.
test("an unknown passage reference adds nothing", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("threadCreate", "Empty", JSON.stringify([{ kind: "heading", text: "Bible" }]), new Date().toISOString());
    s.openSermons("share", "Empty");
  });
  const editor = page.locator('[data-surface="sermon editor"]');
  await expect(editor).toBeVisible();
  await editor.getByRole("button", { name: "Add passage" }).first().click();
  const ask = page.locator('[aria-label="Passage, for example John 3:16 or Ephesians 2:8-10"]');
  await ask.locator("input, textarea").first().fill("Hezekiah 3:16");
  await ask.locator("input, textarea").first().press("Enter");
  await expect(page.locator(".toast, [role=status]").filter({ hasText: "Reference not recognized" })).toBeVisible();
  expect(await outline(page, "Empty")).toEqual(["heading:Bible"]);
});

// Present walks a sermon in its outline's order, headings and points between
// the passages, and the presenter's switch reduces it to the passages alone.
test("Present walks the outline, and the switch hides the presenter's words", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const stamp = new Date().toISOString();
    await s.author("threadCreate", "Walk", JSON.stringify([{ kind: "heading", text: "Bible" }, { kind: "point", text: "first this" }]), stamp);
    await s.author("threadAdd", "Walk", "John 3:16", null, stamp);
    await s.fetchQ("threads");
    s.presentThreadName = "Walk";
    s.showPresent = true;
  });
  const overview = page.locator(".overview");
  await expect(overview).toBeVisible({ timeout: 30_000 });
  await expect(overview.locator(".entry")).toHaveCount(3);
  await expect(overview.locator(".entry.heading .body")).toHaveText("Bible");
  await expect(overview.locator(".entry.bookend .body")).toHaveText("first this");
  await page.getByRole("button", { name: "Hide headings and points" }).click();
  await expect(overview.locator(".entry")).toHaveCount(1);
  await expect(overview.locator(".entry .ref")).toHaveText("John 3:16");
  await page.getByRole("button", { name: "Show headings and points" }).click();
  await expect(overview.locator(".entry")).toHaveCount(3);
});
