import { expect, test, type Page } from "@playwright/test";

// The hymnal: fifth destination, chords, transposition, language, sing mode.
//
// CONTENT-AGNOSTIC ON PURPOSE. The book is still being filled in, and a test
// that names "Amazing Grace" would fail the day a hymn is renumbered and pass
// for the wrong reason the day the file is empty. So every case takes whatever
// the shipped hymnal actually holds and asserts a PROPERTY of it. The one thing
// pinned by name is that the book is not empty — a hymnal.json that failed to
// build would otherwise sail through every case below as a vacuous truth.
//
// Transposition is checked by READING THE CHORDS OFF THE PAGE, not by trusting
// the key label: the engine could report "Bb" in the header while painting the
// old chart, and that is exactly the bug a label-only assertion cannot see.

async function boot(page: Page, vp = { width: 1280, height: 900 }): Promise<void> {
  await page.setViewportSize(vp);
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Open the hymnal and wait for the index to arrive from the engine. */
async function openHymnal(page: Page): Promise<void> {
  await page.evaluate(() => ((window as any).__plumbline.screen = "hymnal"));
  await expect(page.locator('section[aria-label="Hymnal"]')).toBeVisible();
  await expect(page.locator(".row").first()).toBeVisible({ timeout: 60_000 });
}

test("the hymnal is a destination, and it has hymns in it", async ({ page }) => {
  await boot(page);
  await openHymnal(page);
  // Not vacuous: the book shipped with content.
  expect(await page.locator(".row").count()).toBeGreaterThan(5);
  // Every row is numbered and titled — the two things a singer navigates by.
  const first = page.locator(".row").first();
  await expect(first.locator(".num")).not.toBeEmpty();
  await expect(first.locator(".rtitle")).not.toBeEmpty();
});

test("phone: the fifth tab reaches it, and Read comes back", async ({ page }) => {
  await boot(page, { width: 390, height: 780 });
  const bar = page.locator("nav.bottom-nav");
  await expect(bar).toBeVisible();
  await expect(bar.locator("button")).toHaveCount(5);

  await bar.getByRole("button", { name: "Sing" }).click();
  await expect(page.locator('section[aria-label="Hymnal"]')).toBeVisible();
  await bar.getByRole("button", { name: "Read" }).click();
  await expect(page.locator('section[aria-label="Hymnal"]')).toHaveCount(0);
  await expect(page.locator(".pane canvas").first()).toBeVisible();
});

test("the finder matches number, title and first line", async ({ page }) => {
  await boot(page);
  await openHymnal(page);
  const all = await page.locator(".row").count();

  const target = page.locator(".row").first();
  const num = ((await target.locator(".num").textContent()) ?? "").trim();
  const title = ((await target.locator(".rtitle").textContent()) ?? "").trim();
  const find = page.getByLabel("Find a hymn");

  await find.fill(num);
  await expect(page.locator(".row")).toHaveCount(1);
  await expect(page.locator(".row .rtitle")).toHaveText(title);

  // A distinctive slice of the title, not the whole of it — this is a substring
  // match, and matching the whole string would pass even if it were an equality.
  await find.fill(title.slice(1, Math.min(title.length - 1, 12)));
  const byTitle = await page.locator(".row").count();
  expect(byTitle).toBeGreaterThanOrEqual(1);
  expect(byTitle).toBeLessThan(all);

  await find.fill("zzzznotahymn");
  await expect(page.locator(".row")).toHaveCount(0);
  await expect(page.locator(".empty")).toBeVisible();
});

test("the finder narrows by language — code, English name and endonym alike", async ({ page }) => {
  await boot(page);
  await openHymnal(page);
  const all = await page.locator(".row").count();

  // The truth, from the engine: how many hymns carry a German text. Asking the
  // index rather than hard-coding a number keeps this green as the book grows.
  const german = await page.evaluate(async () => {
    const ix = await (window as any).__plumbline.rpc.call("hymnal");
    return (ix?.hymns ?? []).filter((h: any) => Object.keys(h.titles ?? {}).includes("de")).length;
  });
  // Not vacuous, and a real narrowing — the book has German hymns but not only.
  expect(german).toBeGreaterThan(0);
  expect(german).toBeLessThan(all);

  const find = page.getByLabel("Find a hymn");
  // "de" (code), "German" (English name) and "Deutsch" (endonym) all mean the
  // same slice — and case does not matter.
  for (const q of ["de", "German", "deutsch", "DE"]) {
    await find.fill(q);
    await expect(page.locator(".row"), `"${q}" should list every German hymn`).toHaveCount(german);
  }

  // Stacked on the text search: a language token AND a title fragment. Pick a
  // German hymn through the engine and narrow to it by "de" + a title slice.
  const pickTitle = await page.evaluate(async () => {
    const ix = await (window as any).__plumbline.rpc.call("hymnal");
    const h = (ix?.hymns ?? []).find((x: any) => Object.keys(x.titles ?? {}).includes("de"));
    return h?.titles?.de ?? null;
  });
  expect(pickTitle).toBeTruthy();
  const slice = (pickTitle as string).slice(0, 6).toLowerCase();
  await find.fill(`de ${slice}`);
  const combined = await page.locator(".row").count();
  expect(combined).toBeGreaterThanOrEqual(1);
  expect(combined).toBeLessThanOrEqual(german);
});

test("chords appear on request and transpose by a real interval", async ({ page }) => {
  await boot(page);
  await openHymnal(page);
  await page.locator(".row").first().click();
  await expect(page.locator(".stanza").first()).toBeVisible({ timeout: 60_000 });

  // Chords are OFF by default — most people singing are not playing.
  await expect(page.locator(".chord")).toHaveCount(0);

  await page.getByRole("button", { name: "Chords", exact: true }).click();
  const chords = page.locator(".chord");
  await expect(chords.first()).toBeVisible();
  const before = await chords.allTextContents();
  expect(before.length).toBeGreaterThan(2);

  const keyOf = async (): Promise<string> =>
    ((await page.locator(".transpose .key").textContent()) ?? "").trim();
  const startKey = await keyOf();

  // Up two semitones, twice, is a whole tone each time.
  await page.getByRole("button", { name: "Transpose up" }).click();
  await page.getByRole("button", { name: "Transpose up" }).click();
  await expect(page.locator(".transpose .key")).not.toHaveText(startKey);
  const after = await page.locator(".chord").allTextContents();

  // The CHART moved, not just the label — same number of chords, different
  // spellings, and the same shape (a chart that lost its slash basses or its
  // qualities transposed wrongly).
  expect(after.length).toBe(before.length);
  expect(after.join(" ")).not.toBe(before.join(" "));
  const roots = (xs: string[]): string[] => xs.map((c) => c.replace(/^[A-G][#b]?/, ""));
  expect(roots(after)).toEqual(roots(before));

  // And back down is back to exactly where it started, chart and label alike.
  await page.getByRole("button", { name: "Transpose down" }).click();
  await page.getByRole("button", { name: "Transpose down" }).click();
  await expect(page.locator(".transpose .key")).toHaveText(startKey);
  expect(await page.locator(".chord").allTextContents()).toEqual(before);
});

test("a bilingual hymn switches language and keeps the tune's chart", async ({ page }) => {
  await boot(page);
  await openHymnal(page);

  // Find one that ships two languages, through the engine rather than by
  // guessing which hymn it is.
  const two = await page.evaluate(async () => {
    const ix = await (window as any).__plumbline.rpc.call("hymnal");
    return (ix?.hymns ?? []).find((h: any) => Object.keys(h.titles ?? {}).length > 1)?.id ?? null;
  });
  test.skip(!two, "no bilingual hymn in the shipped book yet");

  await page.evaluate((id) => ((window as any).__plumbline.hymn = { id, semis: 0 }), two);
  await expect(page.locator(".stanza").first()).toBeVisible({ timeout: 60_000 });
  await page.getByRole("button", { name: "Chords", exact: true }).click();

  const langs = page.locator(".langs .chip");
  await expect(langs).toHaveCount(2);
  const firstText = await page.locator(".lyric").first().textContent();
  const firstChords = await page.locator(".chord").allTextContents();

  // The OTHER language — the chips render in a fixed order (de before en),
  // so nth(1) can be the one already showing. Clicking the active chip
  // changes nothing and the assertion below would test a no-op.
  await page.locator(".langs .chip:not(.on)").click();
  await expect(page.locator(".lyric").first()).not.toHaveText(firstText ?? "");
  // ONE hymn, one tune: the other language's chart opens on the same chord.
  // (Both texts are sung to the same melody — that is why they are one entry.)
  expect((await page.locator(".chord").allTextContents())[0]).toBe(firstChords[0]);
});

test("sing mode is a fullscreen sunlight surface that Escape leaves", async ({ page }) => {
  await boot(page, { width: 390, height: 780 });
  await openHymnal(page);
  await page.locator(".row").first().click();
  await expect(page.locator(".stanza").first()).toBeVisible({ timeout: 60_000 });

  await page.getByRole("button", { name: "Sing" }).click();
  const host = page.locator(".sing-host");
  await expect(host).toBeVisible();

  // Fullscreen, and stopping above the destination bar rather than under it —
  // the class of bug e2e/surfaces.spec.ts exists for.
  const box = (await host.boundingBox())!;
  const bar = (await page.locator("nav.bottom-nav").boundingBox())!;
  expect(box.x).toBeLessThanOrEqual(1);
  expect(box.width).toBeGreaterThan(388);
  expect(box.y + box.height).toBeLessThanOrEqual(bar.y + 1);

  // Sung type is much larger than the page it came from.
  const size = (sel: string) =>
    page.locator(sel).first().evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
  expect(await size(".sline")).toBeGreaterThan(20);

  // Escape leaves singing without leaving the hymn.
  await page.keyboard.press("Escape");
  await expect(host).toHaveCount(0);
  await expect(page.locator(".stanza").first()).toBeVisible();
});

test("sing mode scrolls itself, and holds still when told to", async ({ page }) => {
  await boot(page, { width: 390, height: 780 });
  await openHymnal(page);
  await page.locator(".row").first().click();
  await expect(page.locator(".stanza").first()).toBeVisible({ timeout: 60_000 });
  await page.getByRole("button", { name: "Sing" }).click();
  await expect(page.locator(".sing-host")).toBeVisible();

  const top = () => page.locator(".sbody").evaluate((el) => el.scrollTop);

  // 0 means hold: a player fretting chords on a short hymn wants the page still.
  await expect(page.locator(".sing-host .key")).toHaveText("hold");
  await page.waitForTimeout(900);
  expect(await top()).toBe(0);

  // Wound up, it creeps — and the creep is continuous, not a jump per line.
  for (let i = 0; i < 6; i++) await page.getByRole("button", { name: "Scroll faster" }).click();
  await page.waitForTimeout(700);
  const mid = await top();
  expect(mid).toBeGreaterThan(0);
  await page.waitForTimeout(700);
  expect(await top()).toBeGreaterThan(mid);

  // And it stops when wound back down, rather than coasting.
  await page.evaluate(() => ((window as any).__plumbline.hymnScroll = 0));
  await page.waitForTimeout(250);
  const stopped = await top();
  await page.waitForTimeout(700);
  expect(await top()).toBe(stopped);
});
