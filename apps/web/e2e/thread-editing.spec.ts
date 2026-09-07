import { expect, test, type Page } from "@playwright/test";

// Threads are edited, not just accumulated: a road gets a verse in the wrong place, or one
// that turned out not to belong. The thread the Share screen walks is a choice too, with the
// stock Romans Road as the default.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** Open the study panel on a thread's detail card, by name. */
async function openThread(page: Page, name: string): Promise<void> {
  await page.evaluate(async (n) => {
    const s = (window as any).__plumbline;
    const i = ((await s.fetchQ("threads"))?.threads ?? []).findIndex((t: any) => t.name === n);
    s.panel = { kind: "thread", index: i };
  }, name);
  await expect(page.locator("aside.panel button.link").first()).toBeVisible({ timeout: 30_000 });
}

/** The refKeys on a thread, in order, straight from the engine. */
async function order(page: Page, name: string): Promise<string[]> {
  return await page.evaluate(async (n) => {
    const s = (window as any).__plumbline;
    const t = ((await s.fetchQ("threads"))?.threads ?? []).find((x: any) => x.name === n);
    return (t?.entries ?? []).map((e: any) => String(e.verse));
  }, name);
}

// Present keyed its verse list by refKey, so a thread holding the same verse twice — which
// nothing forbids — made Svelte throw `each_key_duplicate` and kill the component mid-render,
// leaving a page that would not scroll. Mutation: key that each-block by `e.ref` again — red,
// a page error is thrown and the overview never appears.
test("a thread holding the same verse twice still presents", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await boot(page);

  // A verse the stock Romans Road already carries, added again, plus enough
  // others that the list has to scroll.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const refs = ["Rom 3:23", "Rom 5:8", "Rom 1:16", "Rom 8:1", "Rom 8:28", "Rom 12:1", "Rom 12:2", "Rom 15:13"];
    for (const r of refs) await s.author("threadAdd", "Romans Road", r, null, new Date().toISOString());
    await s.fetchQ("threads");
  });

  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.presentThreadName = s.gospelThread();
    s.showPresent = true;
  });
  await expect(page.locator(".present")).toBeVisible({ timeout: 30_000 });
  const pick = page.locator(".pick").first();
  if (await pick.isVisible().catch(() => false)) await pick.click();

  const overview = page.locator(".overview");
  await expect(overview).toBeVisible({ timeout: 30_000 });
  expect(errors.filter((e) => e.includes("each_key_duplicate")), "Svelte threw on the duplicate verse").toEqual([]);

  // And it scrolls: the list is taller than its box and owns its own overflow.
  const box = await overview.evaluate((el) => ({
    scrollable: el.scrollHeight > el.clientHeight + 2,
    overflowY: getComputedStyle(el).overflowY,
  }));
  expect(box.overflowY).toBe("auto");
  expect(box.scrollable, "the verse list does not overflow its box — add more verses to this test").toBe(true);
  await overview.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  expect(await overview.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
});

// The focused verse stays reachable however long it is, and centred when short. Centring
// comes from auto margins, not `justify-content`: plain `center` pushes an overflowing
// verse's first line above the top edge where scrolling cannot reach it, and `safe center` is
// a keyword WebKit shipped late, so on older iOS the declaration is dropped and short verses
// top-align. Mutations: add `justify-content: center` to `.focus` → the first-line check
// fails while overflowing; drop the auto margins on `.fref`/`.fbody` → the symmetry check
// fails on the short verse.
test("a focused verse scrolls when long and centres when short", async ({ page }) => {
  await boot(page);
  // The KJV's longest verse: the overflow must be real, or every scroll assertion below
  // passes against nothing.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("threadAdd", "Romans Road", "Esth 8:9", null, new Date().toISOString());
    await s.fetchQ("threads");
  });
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.presentThreadName = s.gospelThread();
    s.showPresent = true;
  });
  await expect(page.locator(".present")).toBeVisible({ timeout: 30_000 });
  const pick = page.locator(".pick").first();
  if (await pick.isVisible().catch(() => false)) await pick.click();
  await expect(page.locator(".overview")).toBeVisible({ timeout: 30_000 });
  await page.locator(".entry", { hasText: "Esther" }).click();
  const focus = page.locator(".focus");
  await expect(focus).toBeVisible({ timeout: 20_000 });

  // ── long: a phone in landscape ──
  await page.setViewportSize({ width: 700, height: 300 });
  await expect
    .poll(async () => await focus.evaluate((el) => el.scrollHeight > el.clientHeight + 2), { timeout: 10_000 })
    .toBe(true); // the overflow is real, or nothing below means anything
  const m = await focus.evaluate((el) => {
    el.scrollTo(0, 0);
    const fref = el.querySelector(".fref")!.getBoundingClientRect();
    return {
      overflowY: getComputedStyle(el).overflowY,
      firstLineReachable: fref.top >= el.getBoundingClientRect().top - 1,
    };
  });
  expect(m.overflowY).toBe("auto");
  expect(m.firstLineReachable, "the first line sits above the top edge, unreachable").toBe(true);
  await focus.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  expect(await focus.evaluate((el) => el.scrollTop), "the verse cannot actually be scrolled").toBeGreaterThan(0);

  // ── short: back to a roomy screen, and a verse that fits is centred ──
  await page.setViewportSize({ width: 900, height: 900 });
  await page.locator(".stepbar button", { hasText: "‹" }).click(); // a Romans verse: short
  await expect
    .poll(async () => await focus.evaluate((el) => el.scrollHeight <= el.clientHeight + 2), { timeout: 10_000 })
    .toBe(true);
  const sym = await focus.evaluate((el) => {
    const box = el.getBoundingClientRect();
    const fref = el.querySelector(".fref")!.getBoundingClientRect();
    const fbody = el.querySelector(".fbody")!.getBoundingClientRect();
    return { above: fref.top - box.top, below: box.bottom - fbody.bottom };
  });
  expect(
    Math.abs(sym.above - sym.below),
    `not centred: ${Math.round(sym.above)}px above vs ${Math.round(sym.below)}px below`,
  ).toBeLessThan(30);
});

// Mutation: in links.ts pass `link.entry` instead of `link.entry + link.delta` as the
// destination — red, the order never changes.
test("verses in a thread can be rearranged", async ({ page }) => {
  await boot(page);
  const before = await order(page, "Romans Road");
  expect(before.length).toBeGreaterThan(2);

  // Through the panel's own ↓: the e2e suite runs the production bundle, so the page cannot
  // import `links.ts` to call the dispatcher directly.
  await openThread(page, "Romans Road");
  // The reorder controls live behind the header's edit pencil.
  await page.locator("aside.panel button.link", { hasText: "✎" }).first().click();
  await page.locator("aside.panel button.link", { hasText: "↓" }).first().click();

  await expect.poll(async () => (await order(page, "Romans Road"))[0], { timeout: 20_000 }).toBe(before[1]);
  const after = await order(page, "Romans Road");
  expect(after[1]).toBe(before[0]);
  expect(after.length).toBe(before.length);
});

// Mutation: in `remove_from_thread`, delete the whole thread when its last entry goes — red,
// the thread is missing from the library afterwards.
test("a verse can be removed, and the thread survives", async ({ page }) => {
  await boot(page);
  const before = await order(page, "Romans Road");

  await openThread(page, "Romans Road");
  // ✕ lives behind the edit pencil too.
  await page.locator("aside.panel button.link", { hasText: "✎" }).first().click();
  // Removing asks first, because it cannot be undone, and the dialog's button names the
  // act rather than saying OK — so this clicks the verb.
  await page.locator("aside.panel button.link", { hasText: /^✕$/ }).first().click();
  const confirm = page.locator('[data-surface="confirm"]');
  await expect(confirm).toBeVisible({ timeout: 20_000 });
  // The chapter:verse, not the refKey: the dialog names the passage the way the reader
  // sees it ("Romans 3:23"), and the stored key is "Rom 3:23".
  await expect(confirm).toContainText(before[0].slice(before[0].indexOf(" ") + 1));
  await confirm.locator("button.danger").click();

  await expect.poll(async () => (await order(page, "Romans Road")).length, { timeout: 20_000 }).toBe(before.length - 1);
  expect((await order(page, "Romans Road"))[0]).toBe(before[1]);
  // The thread itself is still there — removing a verse is not deleting a road.
  const names = await page.evaluate(
    async () => ((await (window as any).__plumbline.fetchQ("threads"))?.threads ?? []).map((t: any) => t.name),
  );
  expect(names).toContain("Romans Road");
});

// Mutation: in ShareScreen use the literal "Romans Road" again — red, Share opens the stock
// road instead of the thread the reader chose.
test("Share walks the thread chosen in Settings", async ({ page }) => {
  await boot(page);

  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("threadAdd", "My Gospel Walk", "John 3:16", null, new Date().toISOString());
    await s.fetchQ("threads");
  });

  // Nothing chosen yet: the stock road is the default.
  expect(await page.evaluate(() => (window as any).__plumbline.gospelThread())).toBe("Romans Road");

  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.gospelThread = "My Gospel Walk";
    s.saveConfig();
  });
  expect(await page.evaluate(() => (window as any).__plumbline.gospelThread())).toBe("My Gospel Walk");

  // Share's gospel button opens that thread.
  await page.evaluate(() => ((window as any).__plumbline.screen = "share"));
  await page.getByRole("button", { name: /gospel|Gospel/ }).first().click();
  await expect(page.locator(".present")).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".present .title, .present .name")).toContainText("My Gospel Walk", { timeout: 30_000 });

  // A chosen thread that is later deleted falls back rather than leaving the button dead.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.showPresent = false;
    await s.author("threadRemove", "My Gospel Walk");
    await s.fetchQ("threads");
  });
  await expect
    .poll(async () => await page.evaluate(() => (window as any).__plumbline.gospelThread()), { timeout: 20_000 })
    .toBe("Romans Road");
});

// The overview's rows are <button> flex items in a scrollable column, and a button's
// `min-height: auto` floor doesn't hold — Chromium's button layout reports a one-line minimum
// — so on a phone viewport rows were flex-shrunk to ~40% of their content and every verse
// painted its tail over the entry below. `.entry { flex: none }` is the fix; this pins it at
// the viewport class that exposes it. Mutation: drop `flex: none` from `.entry` and rebuild
// (the CSS ships in the bundle) — red, rows report scrollHeight beyond clientHeight.
test.describe("phone-sized Present", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("overview rows are never shrunk below their verse", async ({ page }) => {
    await boot(page);
    await page.evaluate(async () => {
      const s = (window as any).__plumbline;
      const refs = ["Rom 8:1", "Acts 16:30", "Acts 16:31", "Rom 8:38"];
      for (const r of refs) await s.author("threadAdd", "Romans Road", r, null, new Date().toISOString());
      await s.fetchQ("threads");
    });
    await page.evaluate(() => {
      const s = (window as any).__plumbline;
      s.presentThreadName = "Romans Road";
      s.showPresent = true;
    });
    await expect(page.locator(".overview .entry").first()).toBeVisible({ timeout: 30_000 });

    const m = await page.evaluate(() => {
      const ov = document.querySelector(".overview") as HTMLElement;
      const rows = Array.from(ov.querySelectorAll(".entry")) as HTMLElement[];
      return {
        rows: rows.length,
        // Per row: content the box cannot show. 0 everywhere, or the verse is painting
        // over its neighbour.
        worstShrink: Math.max(...rows.map((el) => el.scrollHeight - el.clientHeight)),
        overflow: ov.scrollHeight - ov.clientHeight,
      };
    });
    // Preconditions against a vacuous pass: enough rows that the column is under real
    // shrink pressure, and a list that genuinely overflows.
    expect(m.rows).toBeGreaterThanOrEqual(9);
    expect(m.overflow, "the overview must have real content to scroll").toBeGreaterThan(200);
    expect(m.worstShrink, "a row is clipping its verse — flex shrink is back").toBeLessThanOrEqual(1);
  });
});


// ── the bookends ────────────────────────────────────────────────────────────

// A thread carries three documents now: `notes` (the author's own scratchpad),
// plus an `opening` read before the first passage and a `closing` after the
// last. Present walks all of it — bookends as their own cards, each entry's
// note under its verse — so a plan of salvation can be shown the way it was
// written rather than as a bare verse list.
//
// The invariant this pins is the ABSENT case, which is the one that goes wrong
// quietly: a thread with no bookends, or one whose bookend was cleared, must
// produce exactly as many steps as it has verses. Nothing here is proved by
// mutation (CLAUDE.md); the reasoning is that both halves are counted against
// the SAME thread. The step count is read off the rendered `.entry` rows, and
// the verse count off the engine, so the two cannot drift together: render a
// blank card for an empty bookend — the obvious way to break this, since
// `opening` is always present on the wire as `""` — and the first expectation
// sees rows > verses while the engine still reports the same entries. A test
// that only checked "the opening card appears when set" would pass against
// exactly that bug.
test("bookends are their own cards, and an absent one takes up no room", async ({ page }) => {
  await boot(page);

  const verses = async (): Promise<number> => (await order(page, "Romans Road")).length;
  const openPresent = async (): Promise<void> => {
    await page.evaluate(() => {
      const s = (window as any).__plumbline;
      s.showPresent = false;
      s.presentThreadName = "Romans Road";
      s.showPresent = true;
    });
    await expect(page.locator(".present")).toBeVisible({ timeout: 30_000 });
    const pick = page.locator(".pick").first();
    if (await pick.isVisible().catch(() => false)) await pick.click();
    await expect(page.locator(".overview")).toBeVisible({ timeout: 30_000 });
  };

  // The stock thread has no bookends: the walk is exactly its verses.
  await openPresent();
  expect(await page.locator(".overview .entry").count()).toBe(await verses());
  expect(await page.locator(".overview .entry.bookend").count()).toBe(0);

  // Both set: two more steps than there are verses, one at each end.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("threadSetOpening", "Romans Road", "Here is where this begins.");
    await s.author("threadSetClosing", "Romans Road", "And here is what it asks.");
    await s.fetchQ("threads");
  });
  await openPresent();
  const rows = page.locator(".overview .entry");
  expect(await rows.count()).toBe((await verses()) + 2);
  await expect(rows.first()).toHaveClass(/bookend/);
  await expect(rows.first()).toContainText("Here is where this begins.");
  await expect(rows.last()).toHaveClass(/bookend/);
  await expect(rows.last()).toContainText("And here is what it asks.");

  // An entry note rides under its own verse, and only there.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("threadEntrySetNote", "Romans Road", 0, "the diagnosis");
    await s.fetchQ("threads");
  });
  await openPresent();
  await expect(page.locator(".overview .entry .note")).toHaveText(["the diagnosis"]);

  // CLEARED — whitespace only — and the cards go with them rather than leaving
  // blanks. This is the half the reader asked for by name.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.author("threadSetOpening", "Romans Road", "   ");
    await s.author("threadSetClosing", "Romans Road", "");
    await s.author("threadEntrySetNote", "Romans Road", 0, "");
    await s.fetchQ("threads");
  });
  await openPresent();
  expect(await page.locator(".overview .entry").count()).toBe(await verses());
  expect(await page.locator(".overview .entry.bookend").count()).toBe(0);
  expect(await page.locator(".overview .entry .note").count()).toBe(0);
});
