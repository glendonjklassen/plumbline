import { expect, test, type Page } from "@playwright/test";

// Boot the app and wait for the reader. On a fresh profile the first-run
// chooser OWNS the screen straight off the loader (the reader mounts after a
// path is chosen — 2026-07-26); take the established path. The title check
// also pins the product branding — index.html, the manifest and the shell
// header must agree.
async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  const established = page.getByRole("button", { name: "Established believer" });
  // Either the chooser (fresh profile) or the reader canvas (returning) —
  // the canvas, not .subtitle, because phones hide the subtitle.
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

test("boots to the reader with the stock set seeded", async ({ page }) => {
  await boot(page);
  await expect(page.locator("canvas").first()).toBeVisible();
  const counts = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const [weaves, threads, tags] = await Promise.all([
      s.engine.weaves(),
      s.engine.threads(),
      s.engine.tags(),
    ]);
    return {
      weaves: weaves?.weaves?.length ?? 0,
      threads: threads?.threads?.length ?? 0,
      tags: tags?.tags?.length ?? 0,
    };
  });
  expect(counts.weaves).toBeGreaterThan(20);
  expect(counts.threads).toBeGreaterThanOrEqual(1);
  // Tags are the reader's own (semantic) groupings — nothing ships stock.
  // This also guards against stray authoring leftovers in the stock set
  // (a shipped amber highlight once painted John 3:7 on every fresh install).
  expect(counts.tags).toBe(0);
});

test("first-run: the welcome owns the boot screen, with no reader behind it", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "New in the faith" })).toBeVisible({ timeout: 90_000 });
  // John 3 used to paint underneath and then get asked a question — the
  // reader must not mount until a path is chosen (feedback 2026-07-26).
  await expect(page.locator(".pane canvas")).toHaveCount(0);
  await expect(page.locator("header .search")).toHaveCount(0);
});

test("first-run: a new believer's welcome reference opens beside John", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New in the faith" }).click({ timeout: 90_000 });
  await expect(page.getByText("We're so glad you've put your faith in Jesus")).toBeVisible();
  await page.getByRole("button", { name: "Psalm 12:6–7" }).click();
  const panes = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { gates: s.gates, panes: s.panes.map((p: any) => ({ book: p.book, chapter: p.chapter, verse: p.targetVerse })) };
  });
  expect(panes.gates).toBe(0); // just the text
  expect(panes.panes[0]).toEqual({ book: "John", chapter: 1, verse: null });
  expect(panes.panes[1]).toEqual({ book: "Ps", chapter: 12, verse: 6 });
});

test("first-run: sharing the gospel lands in the Romans Road presentation", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Sharing the gospel" }).click({ timeout: 90_000 });
  // Straight past the picker into the trail overview.
  await expect(page.locator(".present .title")).toContainText("Romans Road");
  await expect(page.getByText("For all have sinned")).toBeVisible();
});

test("the deferred machine-tier pack loads after boot", async ({ page }) => {
  await boot(page);
  // Boot ships the core pack only (TODO #28); ensureRnd pulls morphology +
  // concept vectors in and re-warms. Force it (instead of waiting out the
  // idle timer) and check a machine-tier lookup lights up.
  const neighbours = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.ensureRnd();
    return (await s.engine.conceptNeighbours("G2316", 3))?.near?.length ?? 0; // G2316 = God
  });
  expect(neighbours).toBeGreaterThan(0);
});

test("menus open promptly after boot (freeze regression)", async ({ page }) => {
  await boot(page);
  // The analytics warm-up must happen behind the splash — if it leaks past
  // boot, this click stalls for seconds and the assertion times out.
  const t0 = Date.now();
  await page.getByLabel("Menu").click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 2_000 });
  expect(Date.now() - t0).toBeLessThan(2_000);
});

test("destinations are exclusive (memorize does not linger)", async ({ page }) => {
  await boot(page);
  await page.getByRole("button", { name: "Memorize" }).click();
  await expect(page.getByText("Review due")).toBeVisible();
  await page.getByRole("button", { name: "Explore", exact: true }).click();
  await expect(page.getByText("Review due")).toBeHidden();
  await expect(page.getByText("Weave map")).toBeVisible();
});

test("word study opens from a single click and respects the gates", async ({ page }) => {
  await boot(page);
  const canvas = page.locator("canvas").first();
  const box = (await canvas.boundingBox())!;
  // Walk the first text line until a word hit opens the panel (single click —
  // Compose tap parity, 2026-07-25; the pin/＋link flow is gone).
  for (const x of [0.3, 0.35, 0.4, 0.45, 0.5]) {
    await canvas.click({ position: { x: box.width * x, y: 46 } });
    if (await page.locator("aside.panel").isVisible().catch(() => false)) break;
  }
  await expect(page.locator("aside.panel")).toBeVisible();
  await expect(page.locator("aside.panel").getByText("your note")).toBeVisible();
});

test("live search shows results and Esc clears", async ({ page }) => {
  await boot(page);
  await page.getByLabel("Search").fill("in the beginning");
  await expect(page.locator("aside.panel")).toContainText("result");
  await page.keyboard.press("Escape");
  await expect(page.locator("aside.panel")).toBeHidden();
});

test("settings switch the theme", async ({ page }) => {
  await boot(page);
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByText("Night (true black)").click();
  const paper = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue("--paper").trim(),
  );
  expect(paper.toLowerCase()).toContain("#0");
});

test("phones keep ONE pane (no split; weaves navigate instead)", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  await expect(page.locator(".nav button[title='Split pane']")).toHaveCount(0);
  // A weave open must navigate the single pane, not split it.
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Explore", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Weaves/ }).click();
  await page.locator("aside.panel button.link").first().click();
  await expect(page.locator(".pane canvas")).toHaveCount(1);
  const panes = await page.evaluate(() => (window as any).__plumbline.panes.length);
  expect(panes).toBe(1);
});

test("the first-run choice survives a relaunch", async ({ page }) => {
  // Config must persist WITHOUT an authoring write — it used to reach
  // IndexedDB only as a side effect of authoring, so a pure reader saw the
  // intro every single launch (2026-07-26).
  await boot(page); // dismisses first-run via the established path
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.getByRole("button", { name: "Established believer" })).toHaveCount(0);
});

test("phones clamp a restored multi-pane session to one pane", async ({ page }) => {
  // A wide session saves a split; reopening on a phone must restore ONE pane.
  // The narrow rule guards addPane, but the restore path must clamp too —
  // 2026-07-26, a phone booting into two panes of John 3.
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.addPane(0);
    s.flushConfig();
    await s.engine.toc(); // FIFO worker queue: the configSave ahead has landed
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  const panes = await page.evaluate(() => (window as any).__plumbline.panes.length);
  expect(panes).toBe(1);
  await expect(page.locator(".pane canvas")).toHaveCount(1);
});

test("passage navigator is two taps, book then chapter, with no waiting", async ({ page }) => {
  // Every grid comes from the boot-prefetched TOC, so the chapter list for
  // any book is on screen immediately. There is no verse step: it used to
  // lay out the whole chapter just to count verses (2026-07-26).
  await boot(page);
  await page.locator(".nav .passage").first().click();
  await page.getByRole("button", { name: "Joel", exact: true }).click();
  // Joel's three chapters render synchronously — no round trip to the engine.
  await expect(page.locator(".grid.nums button")).toHaveCount(3, { timeout: 1_000 });
  await page.getByRole("button", { name: "3", exact: true }).click();
  await expect(page.locator(".subtitle")).toContainText("Joel 3");
});

test("opening a weave splits to its passages; verse clicks stay responsive (freeze regression)", async ({
  page,
}) => {
  await boot(page);
  // Weaves lives inside Explore now (Android parity — no header browse row).
  await page.getByRole("button", { name: "Explore", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Weaves/ }).click();
  await expect(page.locator("aside.panel")).toBeVisible();
  // Open the first weave: both endpoint passages must come up on their own.
  await page.locator("aside.panel button.link").first().click();
  await expect(page.locator(".pane canvas")).toHaveCount(2);
  const targets = await page.evaluate(() =>
    (window as any).__plumbline.panes.map((p: any) => ({
      book: p.book,
      chapter: p.chapter,
      verse: p.targetVerse,
    })),
  );
  expect(targets).toHaveLength(2);
  expect(targets[0].verse).not.toBeNull();
  expect(targets[1].verse).not.toBeNull();
  expect(`${targets[0].book} ${targets[0].chapter}`).not.toBe(`${targets[1].book} ${targets[1].chapter}`);

  // Clicking the card's verse links used to spiral the layout effect into an
  // effect_update_depth_exceeded freeze (~10s) that killed reactivity. Each
  // click must settle fast and leave both panes scrollable.
  const verseLink = page.locator("aside.panel button.link", { hasText: /\d+:\d+/ });
  for (const i of [0, 1]) {
    const t0 = Date.now();
    await verseLink.nth(i).click();
    await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 2_000 });
    expect(Date.now() - t0).toBeLessThan(2_000);
  }
  for (const paneIdx of [0, 1]) {
    const canvas = page.locator(".pane canvas").nth(paneIdx);
    const box = (await canvas.boundingBox())!;
    const before = await page.evaluate(
      (i) => (window as any).__plumbline.panes[i].scrollY,
      paneIdx,
    );
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel(0, before > 0 ? -160 : 160);
    await expect
      .poll(() => page.evaluate((i) => (window as any).__plumbline.panes[i].scrollY, paneIdx))
      .not.toBe(before);
  }
});

test("backup round-trips through a zip", async ({ page }, testInfo) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("John 3:16", "backup probe", "2026-07-25T00:00:00Z");
  });
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Back up (.zip)" }).click(),
  ]);
  const zipPath = testInfo.outputPath("backup.zip");
  await download.saveAs(zipPath);

  // Damage the note, then restore the backup over it.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("John 3:16", "damaged", "2026-07-25T01:00:00Z");
  });
  // Mark the current document, then wait until the restore's reload has
  // actually replaced it (waitForLoadState resolves against the old page).
  await page.evaluate(() => ((window as any).__preRestore = true));
  await page.locator('input[type="file"]').setInputFiles(zipPath);
  await expect
    .poll(async () => page.evaluate(() => (window as any).__preRestore ?? null), {
      timeout: 30_000,
    })
    .toBeNull();
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  const text = await page.evaluate(
    async () => (await (window as any).__plumbline.engine.userNote("John 3:16"))?.text,
  );
  expect(text).toBe("backup probe");
});

// ── boot resilience (2026-07-26) ──────────────────────────────────────────────
// The bugs these cover all shipped: two panes of John 3 on a phone, the intro
// on every launch, a reload that hung on "preparing your study tools", an
// 8.4 s cold parse, and an app that could not actually run offline.

test("boots offline after ONE visit — the whole promise of the thing", async ({ page, context }) => {
  // A first visit must leave the device self-sufficient: someone opens a
  // shared link once, then reads on a plane. The service worker cannot manage
  // this alone (it isn't controlling the page while the shell loads, and it
  // claims the engine worker mid-boot — a race the pack used to lose), so the
  // page and the worker stash their own downloads.
  await boot(page);
  await expect
    .poll(async () => page.evaluate(() => (window as any).__plumbline?.bootTrace?.length ?? 0), {
      timeout: 30_000,
    })
    .toBeGreaterThan(0); // boot finished; precache runs at the following idle
  await page.waitForTimeout(1_500);

  await context.setOffline(true);
  try {
    await page.reload();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 60_000 });
    await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/);
  } finally {
    await context.setOffline(false);
  }
});

test("a first visit never parses the corpus — the pack ships the cache", async ({ page }) => {
  // Every test starts on empty storage, so this IS a first visit. The engine
  // used to parse ~19 MB of JSONL here: 8.4 s on a real phone. The pack now
  // carries a prebuilt idxcache (hydrate `web-cache`, stamped mtime 0 like
  // the browser WASI shim reports) — if that stops shipping or stops
  // validating, the label flips and this fails.
  await boot(page);
  const label = await page.evaluate(async () => {
    const trace: [string, number][] = await (window as any).__plumbline.rpc.bootTrace();
    return trace.find(([stage]) => stage.startsWith("engine open"))?.[0];
  });
  expect(label).toBe("engine open (idxcache fast path)");
});

test("background loading never starves the reader (worker-thread scheduling)", async ({ page }) => {
  // Stage-2 data and the analytics warm-up run on the ONE thread that also
  // answers layout, so they must stay chunked with yields. When they didn't, a
  // pane re-layout queued behind seconds of work and the reader was left
  // half-width after closing a split.
  await boot(page);
  const { worst, chunk } = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    let worst = 0;
    // Cover the window where stage-2 + the warm steps are running.
    for (let i = 0; i < 24; i++) {
      const t0 = performance.now();
      await s.rpc.layout("John", 3, { font: 18, width: 600 + (i % 5), lineSpacing: 1.35, versePerLine: false });
      worst = Math.max(worst, performance.now() - t0);
      await new Promise((r) => setTimeout(r, 120));
    }
    const trace: [string, number][] = await s.rpc.bootTrace();
    const steps = trace.filter(([label]) => label.startsWith("warm step")).map(([, ms]) => ms);
    return { worst, chunk: Math.max(0, ...steps) };
  });
  // Self-calibrating: a queued layout may wait out ONE background chunk, never
  // the whole sequence — so the budget follows this machine's own chunk cost
  // and a slow CI box moves both sides together. A fixed millisecond ceiling
  // does NOT work here: 1500 ms passed against a deliberately un-chunked warm
  // (mutation-tested 2026-07-26 — worst was 311 ms chunked vs 917 ms as one
  // block, with the driver chunk at 357 ms and 223 ms respectively).
  expect(worst).toBeLessThan(Math.max(400, chunk * 2.5));
});

test("the reader scrolls natively, and the pane follows both ways", async ({ page }) => {
  // Scrolling is the browser's (a spacer sized to the chapter, canvas sticky
  // on top) — that is where momentum and fling come from on a phone. The
  // hand-rolled 1:1 pointer version felt dead, so guard the wiring: the
  // scroller must have real range, and scrollTop <-> pane.scrollY must track.
  await boot(page);
  const scroller = page.locator(".pane .scroll").first();
  await expect
    .poll(async () => scroller.evaluate((el) => el.scrollHeight - el.clientHeight))
    .toBeGreaterThan(100);

  // Native scroll → the pane's state.
  await scroller.evaluate((el) => el.scrollTo(0, 320));
  await expect
    .poll(() => page.evaluate(() => Math.round((window as any).__plumbline.panes[0].scrollY)))
    .toBe(320);

  // The pane's state (keyboard, navigation, verse targeting) → native scroll.
  await page.evaluate(() => ((window as any).__plumbline.panes[0].scrollY = 40));
  await expect.poll(async () => scroller.evaluate((el) => Math.round(el.scrollTop))).toBe(40);
});

test("a chapter turn never shows the old text under the new name", async ({ page }) => {
  // The nav strip and header change the instant the reader taps, but the
  // display list arrives from the worker. Holding the previous chapter on the
  // canvas meanwhile put John's text under a header reading Acts, which reads
  // as broken (feedback 2026-07-26). Slowing the layout makes that in-between
  // state observable; the published verse geometry is what the canvas paints.
  await boot(page);
  // The first chapter must actually be ON SCREEN before we navigate away —
  // without this the assertion below passes vacuously against the bug, which
  // is exactly how the first version of this test fooled me.
  await expect
    .poll(() => page.evaluate(() => (window as any).__plumbline.paneVerseGeom[0]?.size ?? 0), {
      timeout: 30_000,
    })
    .toBeGreaterThan(0);

  const during = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const orig = s.rpc.layout.bind(s.rpc);
    s.rpc.layout = (...a: unknown[]) =>
      new Promise((res) => setTimeout(() => orig(...a).then(res), 1_500));
    s.navigate(0, "Rev", 7);
    await new Promise((r) => setTimeout(r, 500)); // inside the slow window
    const staleVerses = s.paneVerseGeom[0]?.size ?? 0;
    s.rpc.layout = orig;
    return { pane: `${s.panes[0].book} ${s.panes[0].chapter}`, staleVerses };
  });
  expect(during.pane).toBe("Rev 7");
  expect(during.staleVerses).toBe(0); // nothing of the previous chapter left
  // …and the header shows the book's NAME, never its OSIS id ("Rev 7").
  await expect(page.locator(".subtitle")).toHaveText("Revelation 7", { timeout: 30_000 });
});
