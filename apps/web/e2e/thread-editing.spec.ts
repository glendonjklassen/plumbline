import { expect, test, type Page } from "@playwright/test";

// THREADS ARE EDITED, not just accumulated (maintainer UAT, 2026-08-18): a road
// gets a verse in the wrong place, or one that turned out not to belong. And
// the thread the Share screen walks is a choice, with the stock Romans Road as
// the default.

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

// THE BUG THE MAINTAINER HIT. Present keyed its verse list by refKey, so a
// thread holding the same verse twice — which nothing forbids, and which
// "I added a couple of verses" produces the moment one is already on the road —
// made Svelte throw `each_key_duplicate` and kill the component mid-render. It
// read as a page that would not scroll and was "all smushed".
//
// MUTATION: key that each-block by `e.ref` again. Red: a page error is thrown
// and the overview never appears.
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

  // And it SCROLLS: the list is taller than its box and owns its own overflow,
  // which is what "smushed" was the absence of.
  const box = await overview.evaluate((el) => ({
    scrollable: el.scrollHeight > el.clientHeight + 2,
    overflowY: getComputedStyle(el).overflowY,
  }));
  expect(box.overflowY).toBe("auto");
  expect(box.scrollable, "the verse list does not overflow its box — add more verses to this test").toBe(true);
  await overview.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  expect(await overview.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
});

// THE FOCUSED VERSE stays reachable however long it is, and stays centred when
// it is short. Centring comes from auto margins, not `justify-content` — plain
// `center` pushes an overflowing verse's first line above the top edge where
// scrolling cannot reach it, and `safe center` is a keyword WebKit shipped late
// (an unsupported keyword drops the declaration, top-aligning short verses on
// exactly the iPhones the PWA is the install path for).
//
// MUTATION (1): add `justify-content: center` to `.focus` → the first-line
// check fails while overflowing. MUTATION (2): drop the auto margins on
// `.fref`/`.fbody` → the symmetry check fails on the short verse.
test("a focused verse scrolls when long and centres when short", async ({ page }) => {
  await boot(page);
  // The KJV's longest verse, added to the road for the occasion: the overflow
  // must be REAL, or every scroll assertion below passes against nothing.
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

// MUTATION: in links.ts pass `link.entry` instead of `link.entry + link.delta`
// as the destination. Red: the order never changes.
test("verses in a thread can be rearranged", async ({ page }) => {
  await boot(page);
  const before = await order(page, "Romans Road");
  expect(before.length).toBeGreaterThan(2);

  // Through the panel's own ↓ — the control the reader taps. The e2e suite runs
  // the production bundle, so the page cannot import `links.ts` to call the
  // dispatcher directly; clicking the rendered control is both possible and a
  // truer test.
  await openThread(page, "Romans Road");
  await page.locator("aside.panel button.link", { hasText: "↓" }).first().click();

  await expect.poll(async () => (await order(page, "Romans Road"))[0], { timeout: 20_000 }).toBe(before[1]);
  const after = await order(page, "Romans Road");
  expect(after[1]).toBe(before[0]);
  expect(after.length).toBe(before.length);
});

// MUTATION: in `remove_from_thread`, delete the whole thread when its last
// entry goes. Red: the thread is missing from the library afterwards.
test("a verse can be removed, and the thread survives", async ({ page }) => {
  await boot(page);
  const before = await order(page, "Romans Road");

  await openThread(page, "Romans Road");
  // Removing ASKS FIRST, because it cannot be undone — the rule `deletethread:`
  // already follows. The dialog names the passage, and its button names the act
  // rather than saying OK, so this clicks the verb.
  await page.locator("aside.panel button.link", { hasText: "remove" }).first().click();
  const confirm = page.locator('[data-surface="confirm"]');
  await expect(confirm).toBeVisible({ timeout: 20_000 });
  // The chapter:verse, not the refKey: the dialog names the passage the way the
  // reader sees it ("Romans 3:23"), and the stored key is "Rom 3:23".
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

// MUTATION: in ShareScreen use the literal "Romans Road" again. Red: Share
// opens the stock road instead of the thread the reader chose.
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

  // Share's gospel button opens THAT thread.
  await page.evaluate(() => ((window as any).__plumbline.screen = "share"));
  await page.getByRole("button", { name: /gospel|Gospel/ }).first().click();
  await expect(page.locator(".present")).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".present .title, .present .name")).toContainText("My Gospel Walk", { timeout: 30_000 });

  // A chosen thread that is later deleted falls back rather than leaving the
  // button dead.
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
