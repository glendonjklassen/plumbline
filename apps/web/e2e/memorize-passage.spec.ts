import { expect, test, type Page } from "@playwright/test";

// A passage is memorized a verse at a time (maintainer, 2026-11-27). It is one
// row of the hub and one place in the queue however long it is; working it is
// its due verses in order, then the first verse not learned yet; a pass opens
// the next verse in the same sitting and Again does not; the next day it starts
// from the beginning with whatever is due. The scheduling itself is proved in
// `crates/core/src/memory/passage/tests.rs` — this drives the screen over it.
//
// Each step can fail against the defect it names, reasoned rather than mutated:
//   - "one row" and the chip's "1 to review" are counts: a passage listed or
//     queued per verse reads 6.
//   - the verse on screen is read from `data-ref`, which is the ref the grade
//     is sent for, and the revealed text is matched on words of verse 1 that
//     verse 2 does not have and the reverse, so a drill over the whole passage
//     (the behaviour this replaces) fails the negative half.
//   - a typed copy of verse 1 scoring 100% fails against a score taken over the
//     passage, where one verse of six is a fraction.
//   - Again is followed by a read of the same ref and Hard by the next one, so
//     a walk that advances on any grade, or on none, fails one or the other.
//   - the files are read from the home: two graded verses are two files, and a
//     card written per verse up front would be six.
//   - day two is built with yesterday's stamps through the same grade endpoint
//     the buttons use. Verse 2 was graded Good and is absent from the walk; a
//     run that replays every learned verse, or resumes at the new verse, fails
//     the asserted order 1, 3, 4, then 5 marked new.
//   - removal is checked in the deletion records, one per verse: a passage
//     removed by its first verse alone looks right on the hub and leaves three
//     cards to come back on the next restore.

const PSALM = {
  one: "The LORD is my shepherd",
  two: "green pastures",
  three: "He restoreth my soul",
};

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

async function openHub(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Study", exact: true }).click();
  await page.locator('[data-lead="memorize"]').click();
  await expect(page.locator('section[aria-label="Memorize"]')).toBeVisible();
}

/** `memory/*.json` and the deletion records, as the home holds them. */
async function home(page: Page): Promise<{ cards: string[]; tombstones: string }> {
  await page.evaluate(() => (window as any).__plumbline.rpc.flush());
  const files: Record<string, string> = await page.evaluate(async () => {
    const out: Record<string, string> = {};
    for (const [path, bytes] of await (window as any).__plumbline.rpc.exportUserData()) {
      out[path] = new TextDecoder().decode(bytes);
    }
    return out;
  });
  return {
    cards: Object.keys(files)
      .filter((p) => p.startsWith("memory/"))
      .sort(),
    tombstones: files["deleted/tombstones.json"] ?? "",
  };
}

const verse = (page: Page) => page.locator('[data-surface="memorize-verse"]');
const button = (page: Page, name: "Again" | "Hard" | "Good" | "Easy") =>
  page.locator(`[data-grade="${name.toLowerCase()}"]`);
const grade = (page: Page, name: "Again" | "Hard" | "Good" | "Easy") => button(page, name).click();

test("a passage is one row, drilled a verse at a time in order", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const err = await s.author("memoryAddPassage", "Ps 23:1", "Ps 23:6", new Date().toISOString());
    if (err) throw new Error(err);
  });

  // Six verses, one thing to do today.
  await expect(page.locator(".plan-chip-row [data-memory]")).toHaveAttribute("data-memory", "1", { timeout: 30_000 });

  await openHub(page);
  const rows = page.locator(".card");
  await expect(rows).toHaveCount(1);
  await expect(rows.first().locator(".ref")).toHaveText("Ps 23:1–6");
  await expect(rows.first().locator(".progress")).toHaveText("0 of 6 verses");
  await page.getByRole("button", { name: "Review due (1)" }).click();

  // Verse 1, alone: named in its passage, marked new, nothing above it.
  await expect(verse(page)).toHaveAttribute("data-ref", "Ps 23:1");
  await expect(page.locator('[data-surface="memorize-passage"]')).toHaveText("Ps 23:1–6");
  await expect(verse(page).locator(".fresh")).toHaveText("New verse");
  await expect(verse(page).locator(".pos")).toHaveText("1 / 1");
  await expect(page.locator('[data-surface="memorize-leadin"]')).toHaveCount(0);
  await page.locator(".reveal summary").click();
  const shown = page.locator(".reveal .drilltext");
  await expect(shown).toContainText(PSALM.one);
  await expect(shown).not.toContainText(PSALM.two);

  // Typed back, verse 1 is the whole answer.
  await page.getByRole("button", { name: "Type it" }).click();
  await page.locator("textarea").fill((await shown.textContent()) ?? "");
  await page.getByRole("button", { name: "Check" }).click();
  await expect(page.locator(".accuracy")).toHaveText("100% recalled");

  // Each button says how well the verse was known and when that brings it
  // back. For a new verse: asked again now, tomorrow, in 2 days, in 4. Read
  // off the buttons, so a caption left blank or the same on all four fails.
  await expect(page.locator(".grades .how")).toHaveText(["None", "Some", "Most", "All"]);
  await expect(page.locator(".grades .when")).toHaveText([
    "Repeat now",
    "Repeat tomorrow",
    "Repeat in 2 days",
    "Repeat in 4 days",
  ]);

  // A pass opens verse 2 in the same sitting, led in by verse 1 in full.
  await grade(page, "Good");
  await expect(verse(page)).toHaveAttribute("data-ref", "Ps 23:2");
  const leadIn = page.locator('[data-surface="memorize-leadin"]');
  await expect(leadIn).toContainText(PSALM.one);
  await expect(leadIn).not.toContainText(PSALM.two);
  await page.locator(".reveal summary").click();
  await expect(shown).toContainText(PSALM.two);
  await expect(shown).not.toContainText(PSALM.one);

  // Again keeps verse 2; only a pass moves on.
  await grade(page, "Again");
  await expect(verse(page)).toHaveAttribute("data-ref", "Ps 23:2");
  await expect(verse(page).locator(".fresh")).toBeVisible();
  await grade(page, "Hard");
  await expect(verse(page)).toHaveAttribute("data-ref", "Ps 23:3");
  await expect(leadIn).toContainText(PSALM.two);

  // Enough for today: the rest of the passage waits.
  await page.getByRole("button", { name: "Stop here" }).click();
  await expect(page.getByText("Done — every card reviewed.")).toBeVisible();

  // Still one row, two verses in; worked today, so nothing is due — and only
  // the verses reached have cards.
  await page.locator(".navbtn").click();
  await expect(rows).toHaveCount(1);
  await expect(rows.first().locator(".progress")).toHaveText("2 of 6 verses", { timeout: 30_000 });
  await expect(page.getByRole("button", { name: "Review due (0)" })).toBeDisabled();
  expect((await home(page)).cards).toEqual(["memory/ps-23-1.json", "memory/ps-23-2.json"]);

  // Going further today is the row's own drill button: it picks up at verse 3.
  await rows.first().locator(".drill").click();
  await expect(verse(page)).toHaveAttribute("data-ref", "Ps 23:3");
  await page.locator(".reveal summary").click();
  await expect(shown).toContainText(PSALM.three);
});

test("the next day a passage starts from the beginning with what is due, then the new verse", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const yesterday = new Date(Date.now() - 24 * 3600 * 1000).toISOString().replace(/\.\d{3}Z$/, "Z");
    const run = async (...args: unknown[]) => {
      const err = await s.author(...args);
      if (err) throw new Error(err);
    };
    await run("memoryAddPassage", "Ps 23:1", "Ps 23:6", yesterday);
    // "Maybe I get through 1–4 today, and 2 I really kill, so I mark it good."
    await run("memoryGrade", "Ps 23:1", "hard", yesterday);
    await run("memoryGrade", "Ps 23:2", "good", yesterday);
    await run("memoryGrade", "Ps 23:3", "hard", yesterday);
    await run("memoryGrade", "Ps 23:4", "hard", yesterday);
  });

  await openHub(page);
  await expect(page.locator(".card")).toHaveCount(1);
  await expect(page.locator(".card .progress")).toHaveText("4 of 6 verses");
  await page.getByRole("button", { name: "Review due (1)" }).click();

  // 1, 3, 4 — reviews, so not marked new — and verse 2 sits today out.
  for (const ref of ["Ps 23:1", "Ps 23:3", "Ps 23:4"]) {
    await expect(verse(page)).toHaveAttribute("data-ref", ref);
    await expect(verse(page).locator(".fresh")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Stop here" })).toHaveCount(0);
    // A review that is missed comes back tomorrow, not now. And no two
    // buttons say the same: "6 days" under all three passes was the defect.
    await expect(page.locator(".grades .when")).toHaveText([
      "Repeat tomorrow",
      "Repeat in 3 days",
      "Repeat in 6 days",
      "Repeat in 10 days",
    ]);
    // Four across at this width, not three and one.
    const tops = await page.locator(".grades button").evaluateAll((els) => els.map((e) => e.getBoundingClientRect().top));
    expect(new Set(tops).size).toBe(1);
    await grade(page, "Good");
  }
  // Then the next new verse, led in by the one before it.
  await expect(verse(page)).toHaveAttribute("data-ref", "Ps 23:5");
  await expect(verse(page).locator(".fresh")).toHaveText("New verse");
  await expect(page.locator('[data-surface="memorize-leadin"]')).toContainText("valley of the shadow of death");
  await grade(page, "Good");
  await expect(verse(page)).toHaveAttribute("data-ref", "Ps 23:6");
  await grade(page, "Good");
  // The last verse passed ends the passage, and with it the queue.
  await expect(page.getByText("Done — every card reviewed.")).toBeVisible();

  await page.locator(".navbtn").click();
  await expect(page.locator(".card .progress")).toHaveText("6 of 6 verses", { timeout: 30_000 });
  expect((await home(page)).cards).toHaveLength(6);

  // Removing the passage removes every verse's card, each with its record.
  await page.locator(".card .remove").click();
  await page.locator('[data-surface="confirm"] button.danger').click();
  await expect(page.locator(".card")).toHaveCount(0, { timeout: 30_000 });
  const after = await home(page);
  expect(after.cards).toEqual([]);
  for (let v = 1; v <= 6; v++) {
    expect(after.tombstones, `verse ${v} has a deletion record`).toContain(`card:ps-23-${v}.json`);
  }
});

test("a single verse beside a passage is its own row, and each is one place in the queue", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const now = new Date().toISOString();
    for (const args of [
      ["memoryAddPassage", "Ps 23:1", "Ps 23:6", now],
      ["memoryAdd", "John 3:16", now],
    ]) {
      const err = await s.author(...args);
      if (err) throw new Error(err);
    }
  });
  await openHub(page);
  await expect(page.locator(".card .ref")).toHaveText(["Ps 23:1–6", "John 3:16"]);
  // The single verse has no verse count to show.
  await expect(page.locator(".card").nth(1).locator(".progress")).toHaveCount(0);
  await page.getByRole("button", { name: "Review due (2)" }).click();

  // Row 1 of 2: the passage's first verse. Stopping there moves to row 2,
  // the single verse — no passage label, nothing to stop.
  await expect(verse(page)).toHaveAttribute("data-ref", "Ps 23:1");
  await expect(verse(page).locator(".pos")).toHaveText("1 / 2");
  await grade(page, "Good");
  await expect(verse(page)).toHaveAttribute("data-ref", "Ps 23:2");
  await expect(verse(page).locator(".pos")).toHaveText("1 / 2");
  await page.getByRole("button", { name: "Stop here" }).click();
  await expect(verse(page)).toHaveAttribute("data-ref", "John 3:16");
  await expect(verse(page).locator(".pos")).toHaveText("2 / 2");
  await expect(page.locator('[data-surface="memorize-passage"]')).toHaveCount(0);
  await expect(page.locator('[data-surface="memorize-leadin"]')).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Stop here" })).toHaveCount(0);
  await grade(page, "Good");
  await expect(page.getByText("Done — every card reviewed.")).toBeVisible();
});
