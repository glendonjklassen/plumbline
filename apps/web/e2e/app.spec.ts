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
  await expect(page.getByText("I'm so glad you've put your faith in Jesus")).toBeVisible();
  await page.getByRole("button", { name: "Psalm 12:6–7" }).click();
  const panes = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { gates: s.gates, panes: s.panes.map((p: any) => ({ book: p.book, chapter: p.chapter, verse: p.targetVerse })) };
  });
  expect(panes.gates).toBe(0); // just the text
  expect(panes.panes[0]).toEqual({ book: "John", chapter: 1, verse: null });
  expect(panes.panes[1]).toEqual({ book: "Ps", chapter: 12, verse: 6 });
});

test("first-run: sharing the gospel asks for your church, then opens the Romans Road", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Sharing the gospel" }).click({ timeout: 90_000 });
  // The reader about to hand this to someone is asked for their church, and
  // told why (2026-07-27) — it is optional, and skipping goes straight on.
  await expect(page.getByText("Before you share it")).toBeVisible();
  await expect(page.getByText(/links and QR codes you share carry it/)).toBeVisible();
  await page.getByPlaceholder("Church name").fill("Grace Bible Church");
  await page.getByPlaceholder(/When and where/).fill("Sundays 10am, 12 Long Street");
  await page.getByRole("button", { name: "Open the Romans Road" }).click();
  await expect(page.locator(".present .title")).toContainText("Romans Road");
  await expect(page.getByText("For all have sinned")).toBeVisible();

  // What they typed is now theirs, and rides along in what they share.
  const church = await page.evaluate(() => (window as any).__plumbline.church);
  expect(church.name).toBe("Grace Bible Church");
  expect(church.info).toBe("Sundays 10am, 12 Long Street");
});

test("a shared link carries the church, and the welcome names them", async ({ page }) => {
  // The whole point of the query string: one QR hands over the Bible AND the
  // people who sent it (2026-07-27).
  await page.goto("/?church=Grace+Bible+Church&churchInfo=Sundays+10am&churchUrl=https%3A%2F%2Fexample.org");
  await expect(page.getByText("Shared with you by")).toBeVisible({ timeout: 90_000 });
  await expect(page.getByText("Grace Bible Church")).toBeVisible();
  await expect(page.getByText("Sundays 10am")).toBeVisible();

  // The address bar is left clean — a bookmark of this is the app, not a
  // link about somebody's church.
  expect(await page.evaluate(() => location.search)).toBe("");

  // Saved as this reader's own, so THEIR shares carry it onward.
  const church = await page.evaluate(() => (window as any).__plumbline.church);
  expect(church).toEqual({ name: "Grace Bible Church", info: "Sundays 10am", url: "https://example.org" });

  // And it survives a relaunch. (Finish first-run first — the welcome owns
  // the screen until a path is chosen, so a reload before that just shows it
  // again.)
  await page.getByRole("button", { name: "Established believer" }).click();
  await page.getByRole("button", { name: "Start reading" }).click();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  const after = await page.evaluate(() => (window as any).__plumbline.church.name);
  expect(after).toBe("Grace Bible Church");
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

// A PHONE must never be asked to approve the analysis pack. Deferring it keeps
// it off the boot path; it was also keeping it out of the session entirely, so
// every launch put a "one-time ~4 MB download / Load analysis" button in front
// of a reader who had already taken that download — and, on the same screen, a
// second notice underneath about a slow first read (feedback 2026-07-27, with a
// screenshot). It loads itself now.
test("a phone is never asked to approve the analysis pack", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 }); // before goto: deferRnd is read at boot
  await boot(page);
  expect(await page.evaluate(() => (window as any).__plumbline.rndDeferred)).toBe(false);

  // It arrives without anyone tapping anything.
  await page.waitForFunction(() => (window as any).__plumbline.rndState === "ready", null, {
    timeout: 90_000,
  });
  const neighbours = await page.evaluate(
    async () => ((await (window as any).__plumbline.engine.conceptNeighbours("G2316", 3))?.near ?? []).length,
  );
  expect(neighbours).toBeGreaterThan(0);

  // And the offer never appeared.
  await expect(page.getByRole("button", { name: "Load analysis" })).toHaveCount(0);
  await expect(page.getByText("one-time ~4 MB download")).toHaveCount(0);
});

// The two stacked notices in that screenshot: the pack's own progress line, and
// underneath it the "first one takes a few seconds" note. One wait deserves one
// explanation. The study must genuinely be STUCK for this to mean anything —
// the slow-read note only arms after 600ms of unanswered blocks, so a version
// of this test that asserted straight away passed with the guard removed. A
// null refKey leaves the read unanswered for real.
test("a loading study explains itself once, not twice", async ({ page }) => {
  await boot(page);
  // Let the background load settle FIRST — otherwise it lands mid-test and
  // flips rndState back to "ready" under us, which is what made an earlier
  // version of this test fail for the wrong reason.
  await page.waitForFunction(() => (window as any).__plumbline.rndState === "ready", null, {
    timeout: 90_000,
  });
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.rndState = "loading";
    s.rndDeferred = true;
    s.panel = { kind: "wordStudy", refKey: null, tokenIndex: 0 };
  });
  await expect(page.locator(".rnd-note")).toBeVisible();
  await page.waitForTimeout(1200); // well past SLOW_READ_MS
  await expect(page.locator(".loading")).toBeVisible(); // still unanswered, so
  await expect(page.locator(".firstslow")).toHaveCount(0); // ...and only ONE notice

  // Control: the same stuck read DOES explain itself once the pack is in — the
  // guard suppresses the note during the download, it doesn't delete it.
  await page.evaluate(() => ((window as any).__plumbline.rndState = "ready"));
  await expect(page.locator(".firstslow")).toBeVisible();
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
  await page.getByRole("searchbox").fill("in the beginning");
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

test("Settings can make the app completely offline, and says when it is", async ({ page }) => {
  // The reader's answer to "will this work with no signal?". Most of the app
  // is local after a first visit; this verifies every pack file is really in
  // the offline cache and fetches whatever isn't (a failed download or an
  // eviction otherwise goes unnoticed until the reader has no connection).
  await boot(page);
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  const download = page.getByRole("button", { name: "Download everything" });
  if (await download.isVisible().catch(() => false)) await download.click();
  await expect(page.getByText("Everything is on this device")).toBeVisible({ timeout: 120_000 });

  // Not just a label: the survey must agree that nothing is missing.
  const missing = await page.evaluate(async () => {
    const manifest = await (await fetch("pack/manifest.json")).json();
    const cache = await caches.open("plumbline-v1");
    let missing = 0;
    for (const f of manifest.files)
      if (!(await cache.match(`pack/${f.path}.gz?v=${manifest.version}`, { ignoreVary: true }))) missing++;
    return missing;
  });
  expect(missing).toBe(0);
});

test("the welcome's verses are the corpus text, verbatim and instant", async ({ page }) => {
  // The welcome quotes scripture from literals rather than asking the engine
  // for ten verses one at a time — they used to pop in a beat after the page
  // (feedback 2026-07-27). A copy can drift, so this compares every quote on
  // screen against the corpus itself.
  await page.goto("/");
  await page.getByRole("button", { name: "New in the faith" }).click({ timeout: 90_000 });
  await expect(page.getByText("I'm so glad you've put your faith in Jesus")).toBeVisible();

  // The quotes are present in the very first paint of this screen, not filled
  // in later: no blockquote may be empty at any point.
  const quotes = await page.locator("blockquote .vq-text").allInnerTexts();
  expect(quotes.length).toBeGreaterThan(5);
  for (const q of quotes) expect(q.replace(/[“”]/g, "").trim().length).toBeGreaterThan(20);

  const expected = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const groups = [
      ["Ps 12:6", "Ps 12:7"],
      ["Heb 10:24", "Heb 10:25"],
      ["Ps 119:11"],
      ["Rom 5:8", "John 3:16"],
      ["John 10:28", "1John 5:13"],
      ["Phil 1:6", "1John 1:9"],
      ["2Tim 3:16", "2Tim 3:17"],
      ["Ps 34:18"],
    ];
    const out: string[] = [];
    for (const g of groups) {
      const parts: string[] = [];
      for (const k of g) parts.push((await s.engine.verse(k))?.body ?? `MISSING ${k}`);
      out.push(parts.join(" "));
    }
    return out;
  });
  expect(quotes.map((q) => q.replace(/[“”]/g, "").trim())).toEqual(expected);
});

test("the share QR encodes the church, not just the app", async ({ page }) => {
  // One scan should hand over both (2026-07-27). The QR is generated at
  // render time now, so setting a church must change what it encodes — a
  // longer payload needs a bigger symbol.
  await boot(page);
  const modulesFor = async () =>
    page.locator(".share-dialog svg").getAttribute("viewBox").then((v) => Number(v!.split(" ")[2]));

  await page.getByRole("button", { name: "Share", exact: true }).click();
  await expect(page.locator(".share-dialog")).toBeVisible();
  const plain = await modulesFor();
  // The dialog shows the HOST, never the full link: with a church attached
  // the URL runs off a phone screen (feedback 2026-07-27).
  await expect(page.locator(".share-url")).toHaveText("plumblinebible.org");
  await page.getByRole("button", { name: "Close" }).click();

  await page.evaluate(() =>
    (window as any).__plumbline.setChurch({
      name: "Grace Bible Church",
      info: "Sundays 10am, 12 Long Street",
      url: "https://example.org",
    }),
  );
  await page.getByRole("button", { name: "Share", exact: true }).click();
  const withChurch = await modulesFor();
  expect(withChurch).toBeGreaterThan(plain); // more to encode, bigger symbol
  await expect(page.locator(".share-with")).toHaveText("with Grace Bible Church");
  await expect(page.locator(".share-url")).toHaveText("plumblinebible.org");

  // The link itself — what the QR encodes and "Share the link" hands over.
  const links = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { normal: s.shareLink, present: s.presentShareLink };
  });
  expect(links.normal).toContain("church=Grace+Bible+Church");
  expect(links.normal).toContain("churchInfo=Sundays+10am");
  // An ordinary share is an ordinary link — it must NOT declare the recipient
  // a new believer; that is only for what Present hands over.
  expect(links.normal).not.toContain("start=new");
  expect(links.present).toContain("start=new");
  expect(links.present).toContain("church=Grace+Bible+Church");
});

// Sharing a PASSAGE is a QR carrying the passage, not the phone's share sheet
// carrying a wall of text (feedback 2026-07-27). Present is held up to someone
// in front of you, so what they scan must land them in the reader at the verse.
test("Present shares the passage as a QR whose link opens at the first verse", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Sharing the gospel" }).click({ timeout: 90_000 });
  await page.getByRole("button", { name: "Open the Romans Road" }).click();
  await expect(page.locator(".present .title")).toContainText("Romans Road");

  // Record what the copy button hands over, without needing clipboard perms.
  await page.evaluate(() => {
    (window as any).__copied = [];
    navigator.clipboard.writeText = async (t: string) => void (window as any).__copied.push(t);
  });

  // Present's own Share, not the header's.
  await page.locator(".present .sharebtn").click();
  // A QR, and no share-sheet text dump.
  await expect(page.locator(".sharesheet svg")).toBeVisible();
  await page.getByRole("button", { name: "Copy the passages" }).click();

  const copied: string = await page.evaluate(() => (window as any).__copied[0]);
  // The link carries the thread's FIRST verse, url-encoded ("Rom 3:23").
  expect(copied).toMatch(/[?&]at=Rom\+3%3A23/);
  expect(copied).toContain("For all have sinned");
});

// The receiving half of that QR: the link must actually land on the verse.
test("a shared passage link opens the reader at its verse", async ({ page }) => {
  await page.goto("/?at=Ps+23%3A1");
  await page.getByRole("button", { name: "Established believer" }).click({ timeout: 90_000 });
  await page.getByRole("button", { name: "Start reading" }).click();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

  const where = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return s.panes.map((p: any) => ({ book: p.book, chapter: p.chapter, verse: p.targetVerse }));
  });
  expect(where[0].book).toBe("Ps");
  expect(where[0].chapter).toBe(23);
  // And the address bar is left clean, like every other shared parameter.
  expect(await page.evaluate(() => location.search)).toBe("");
});

// Every versioned URL is content-addressed, so an update ADDS an entry beside
// the old one — and nothing used to remove the old. Three data updates meant
// three whole ~12 MB packs stranded on the device (2026-07-27).
test("updating sweeps the versions this build no longer uses", async ({ page }) => {
  await boot(page);
  // Let the real boot-time sweep finish first, so this test's seeding isn't
  // racing it and the counts below are its own doing.
  await page.evaluate(() => (window as any).__plumbline.sweepCaches());

  const before = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const c = await caches.open("plumbline-v1");
    const put = (u: string) => c.put(new Request(u), new Response("x"));
    // A whole previous pack, a previous wasm, and a superseded hashed bundle.
    await put(location.origin + "/pack/data/kjv.jsonl.gz?v=OLDPACK");
    await put(location.origin + "/pack/manifest.json?v=OLDPACK");
    await put(location.origin + "/plumbline_ffi.wasm?v=OLDBUILD");
    await put(location.origin + "/assets/index-DEADBEEF.js");
    // Entries that must survive: un-versioned, and the CURRENT pack version.
    await put(location.origin + "/index.html");
    await put(location.origin + `/pack/data/keep.gz?v=${s.packVersion}`);
    return (await c.keys()).map((r: Request) => r.url.replace(location.origin, ""));
  });
  expect(before).toContain("/pack/data/kjv.jsonl.gz?v=OLDPACK");
  expect(before).toContain("/assets/index-DEADBEEF.js");

  await page.evaluate(() => (window as any).__plumbline.sweepCaches());

  const after = await page.evaluate(async () => {
    const c = await caches.open("plumbline-v1");
    return (await c.keys()).map((r: Request) => r.url.replace(location.origin, ""));
  });
  // Superseded versions gone...
  expect(after).not.toContain("/pack/data/kjv.jsonl.gz?v=OLDPACK");
  expect(after).not.toContain("/pack/manifest.json?v=OLDPACK");
  expect(after).not.toContain("/plumbline_ffi.wasm?v=OLDBUILD");
  expect(after).not.toContain("/assets/index-DEADBEEF.js");
  // ...and nothing else was collateral. The un-versioned shell entry and the
  // current pack version both stay, or the next launch is broken/offline-dead.
  expect(after).toContain("/index.html");
  const current = await page.evaluate(() => (window as any).__plumbline.packVersion);
  expect(after).toContain(`/pack/data/keep.gz?v=${current}`);
  // The bundle this page is actually running must still be cached.
  const running = await page.evaluate(
    () => document.querySelector<HTMLScriptElement>('script[type="module"][src*="/assets/"]')!.src,
  );
  expect(after).toContain(new URL(running).pathname);
});

// The update toast: a deploy landed while this session stayed open. Driven
// through the real checker with a stubbed index.html, so it exercises the
// bundle comparison rather than just the flag.
test("a new deploy offers an update, and only when the build really changed", async ({ page }) => {
  await boot(page);
  const realFetch = "__realFetch";
  await page.evaluate((k) => {
    (window as any)[k] = window.fetch.bind(window);
  }, realFetch);

  // Same build deployed → no toast. This is the guard that matters: a checker
  // that always fires would nag every reader on every resume.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const mine = document.querySelector<HTMLScriptElement>('script[type="module"][src*="/assets/"]')!.src;
    const name = mine.split("/").pop();
    window.fetch = async () => new Response(`<script type="module" src="/assets/${name}"><\/script>`);
    await s.checkForUpdate(true);
  });
  await expect(page.locator(".toast.update")).toHaveCount(0);

  // A different hashed bundle → the offer appears.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    window.fetch = async () =>
      new Response('<script type="module" src="/assets/index-NEWBUILD.js"><\/script>');
    await s.checkForUpdate(true);
  });
  await expect(page.locator(".toast.update")).toBeVisible();
  await expect(page.locator(".toast.update")).toContainText("A new version is ready");

  // Dismissing leaves the reader where they were — no reload.
  await page.locator(".toast.update .dismiss").click();
  await expect(page.locator(".toast.update")).toHaveCount(0);

  // Taking it reloads.
  await page.evaluate(() => (window as any).__plumbline.checkForUpdate(true));
  await expect(page.locator(".toast.update")).toBeVisible();
  await Promise.all([
    page.waitForEvent("framenavigated"),
    page.locator(".toast.update .upd").click(),
  ]);
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
});

// A stranger's query string is untrusted input: a nonsense `at` must be rejected
// OUTRIGHT, never handed to navigation. The reader staying on John proves little
// on its own — the link dispatcher discards an unparseable ref anyway — so the
// signal is the address bar: the shell only strips the query once it has
// consumed something from it, so junk left in place means junk never counted.
test("a bogus at= parameter is rejected, not merely survived", async ({ page }) => {
  await page.goto("/?at=javascript%3Aalert(1)");
  await page.getByRole("button", { name: "Established believer" }).click({ timeout: 90_000 });
  await page.getByRole("button", { name: "Start reading" }).click();
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });

  const book = await page.evaluate(() => (window as any).__plumbline.panes[0].book);
  expect(book).toBe("John"); // the default landing, untouched
  expect(await page.evaluate(() => location.search)).toContain("at=");
});

test("a Present link names the church and drops the setup paths", async ({ page }) => {
  // Present is the screen you show someone face to face: its link says who it
  // was meant for, so the welcome offers only the two paths that fit and
  // still names whoever handed it over.
  await page.goto("/?church=Grace+Bible+Church&start=new");
  await expect(page.getByText("Shared with you by")).toBeVisible({ timeout: 90_000 });
  await expect(page.getByRole("button", { name: "New in the faith" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Curious about the Bible" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Established believer" })).toHaveCount(0);
  expect(await page.evaluate(() => location.search)).toBe("");
});

test("first-run: curious about the Bible is its own path, and stays re-readable", async ({ page }) => {
  // A third way in, for someone who hasn't decided what they believe
  // (2026-07-27) — and the welcome a reader was given must be readable again
  // afterwards, from the chrome rather than by reinstalling.
  await page.goto("/");
  await page.getByRole("button", { name: "Curious about the Bible" }).click({ timeout: 90_000 });
  await expect(page.getByText("I'm glad you're curious about the Bible")).toBeVisible();
  await expect(page.getByText(/help thou mine unbelief/)).toBeVisible();
  await expect(page.getByText(/contrite spirit/)).toBeVisible(); // the struggles verse
  await page.getByRole("button", { name: "Open the book of John" }).click();
  await expect(page.locator(".subtitle")).toContainText("John 1");

  // Back to it from the top bar, without changing anything.
  await page.getByRole("button", { name: "Welcome" }).click();
  await expect(page.getByText("I'm glad you're curious about the Bible")).toBeVisible();
  await page.getByRole("button", { name: "Close" }).click();
  await expect(page.locator(".pane canvas").first()).toBeVisible();

  // …and it survives a relaunch, since it's the reader's own welcome now.
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.getByRole("button", { name: "Welcome" })).toBeVisible();
});

test("a Present link offers only the two paths it was meant for", async ({ page }) => {
  // Handed to someone in person: new believer or curious. Setting up study
  // tiers is not what that moment is for.
  await page.goto("/?start=new");
  await expect(page.getByRole("button", { name: "New in the faith" })).toBeVisible({ timeout: 90_000 });
  await expect(page.getByRole("button", { name: "Curious about the Bible" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Established believer" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Sharing the gospel" })).toHaveCount(0);
});

test("the phone top bar stays on one row, search behind a glass", async ({ page }) => {
  // Welcome + Church + Share + Search + ≡ wrapped onto a second row on a
  // phone (feedback 2026-07-27). Search collapses to an icon and only takes
  // the row while it is being used.
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  await page.evaluate(() =>
    (window as any).__plumbline.setChurch({ name: "Grace Bible Church", info: "", url: "https://example.org" }),
  );
  const oneRow = () =>
    page.locator("header").evaluate((h) => {
      const top = h.getBoundingClientRect().top;
      return [...h.children].every((c) => Math.abs(c.getBoundingClientRect().top - top) < 24);
    });
  await expect.poll(oneRow).toBe(true);

  // The field is behind the glass, and taking it doesn't push anything off.
  await expect(page.getByRole("searchbox")).toBeHidden();
  await page.getByLabel("Open search").click();
  await expect(page.getByRole("searchbox")).toBeFocused();
  expect(await oneRow()).toBe(true);
  await page.getByRole("searchbox").fill("in the beginning");
  await expect(page.locator("aside.panel")).toContainText("result");
  await page.getByLabel("Close search").click();
  await expect(page.getByRole("searchbox")).toBeHidden();
});

test("the welcome points a new believer at the church that shared it", async ({ page }) => {
  // "Find a church" used to say "consider reaching out to them" in the
  // abstract, even when the link named the church (feedback 2026-07-27).
  await page.goto(
    "/?church=Grace+Bible+Church&churchInfo=Sundays+10AM&churchUrl=https%3A%2F%2Fexample.org&start=new",
  );
  await page.getByRole("button", { name: "New in the faith" }).click({ timeout: 90_000 });
  const findChurch = page.locator(".welcome p", { hasText: "Find a church" });
  await expect(findChurch).toContainText("shared with you by");
  await expect(findChurch).toContainText("Grace Bible Church");
  await expect(findChurch).toContainText("Sundays 10AM");
  await expect(findChurch.getByRole("link", { name: /Visit Grace Bible Church/ })).toHaveAttribute(
    "href",
    "https://example.org/",
  );
});

test("with no church shared, the welcome keeps its general advice", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New in the faith" }).click({ timeout: 90_000 });
  const findChurch = page.locator(".welcome p", { hasText: "Find a church" });
  await expect(findChurch).toContainText("If someone shared this app with you");
  await expect(findChurch).not.toContainText("shared with you by");
});

// The Check button used to read the engine through `session.engine`, the
// console/e2e proxy, which returns a PROMISE — so `score.accuracy` was
// undefined and every check reported "0% recalled", even a verbatim
// copy/paste (feedback 2026-07-27). Drives the real UI, so a regression in
// either the wiring or the scoring fails here.
test("checking a typed recall scores it (a perfect copy is 100%)", async ({ page }) => {
  await boot(page);
  // Seed a card for a known verse and drill it straight away.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.memoryAdd("John 3:16", new Date().toISOString());
  });
  await page.getByRole("button", { name: "Memorize" }).click();
  await page.getByRole("button", { name: "Review due", exact: false }).click();
  await page.getByRole("button", { name: "Type it" }).click();

  // Reveal the verse and type back exactly what the drill asks for.
  const drilled = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return (await s.engine.memoryDrill("John 3:16", 0))?.text as string;
  });
  expect(drilled).toContain("For God so loved");
  await page.locator("textarea").fill(drilled);
  await page.getByRole("button", { name: "Check" }).click();
  await expect(page.locator(".accuracy")).toHaveText("100% recalled");

  // A SECOND check must rescore rather than leave the first score on screen.
  // Pinned to the exact figure on purpose: `not.toHaveText("100% recalled")`
  // also passes when `.accuracy` has gone MISSING, so it would greet a check
  // that silently cleared the score as a success. Of the 25 words of John
  // 3:16 this wrong answer shares only "the" — 1/25 = 4%.
  await page.locator("textarea").fill("nothing like the verse at all");
  await page.getByRole("button", { name: "Check" }).click();
  await expect(page.locator(".accuracy")).toHaveText("4% recalled");
});

// A reader who pauses to think must not lose their work. `nowStamp()` is
// second-granularity and lands in the read-through cache KEY, so the due-list
// read minted a fresh key every second, fell back to [], and re-ran the reset
// effect — clearing the textarea and dropping the mode back to "First letters"
// about once a second, which made typed recall unusable (feedback 2026-07-27).
// The dwell is a fixed 2.5s on purpose: it is not a performance budget but a
// span that must straddle the one-second boundary the churn ran on.
test("a typed recall survives a pause and a background study refresh", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.memoryAdd("John 3:16", new Date().toISOString());
  });
  await page.getByRole("button", { name: "Memorize" }).click();
  await page.getByRole("button", { name: "Review due", exact: false }).click();
  await page.getByRole("button", { name: "Type it" }).click();

  const drilled = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return (await s.engine.memoryDrill("John 3:16", 0))?.text as string;
  });
  await page.locator("textarea").fill(drilled);

  await page.waitForTimeout(2_500);
  await expect(page.locator("textarea")).toHaveValue(drilled);
  await expect(page.locator(".modes button", { hasText: "Type it" })).toHaveClass(/checked/);
  await expect(page.locator(".pos")).toHaveText("1 / 1");

  // Authoring landing mid-drill drops every cached study read. The drill must
  // keep its place, its mode and its text through that too.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.memoryAdd("Ps 1:1", new Date().toISOString());
  });
  await expect(page.locator("textarea")).toHaveValue(drilled);
  await expect(page.locator(".pos")).toHaveText("1 / 1");

  // Still live, not merely frozen: it scores.
  await page.getByRole("button", { name: "Check" }).click();
  await expect(page.locator(".accuracy")).toHaveText("100% recalled");
});

// Through the PICKER, not the engine. The test below seeds its card by calling
// memoryAddPassage directly, which sailed straight past a dead commit button:
// commit() read `start`/`throughRef` AFTER close() had nulled the state they
// derive from, so the engine got null for both and every attempt toasted "null
// or invalid argument" with no card written (feedback 2026-07-27).
test("the passage picker actually files the card it names", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    (window as any).__plumbline.memorizePassageFrom = "Ps 23:1";
  });
  await expect(page.getByText("Tap the verse this passage ends on.")).toBeVisible();
  await page.locator(".sheet .grid button", { hasText: /^3$/ }).click();
  await page.getByRole("button", { name: /^Memorize Ps 23:1/ }).click();

  // The toast names the passage rather than reporting an engine error...
  await expect(page.locator(".toast")).toHaveText("Memorizing Ps 23:1–3");
  // ...and the card is really on disk, spanning the range that was picked.
  const cards = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const cov = await s.engine.memoryCoverage(new Date().toISOString());
    return (cov?.cards ?? []).map((c: any) => c.label ?? c.ref);
  });
  expect(cards).toEqual(["Ps 23:1–3"]);
});

// A whole section as ONE card (2026-07-27): the hub lists one row labelled with
// the range, and the drill covers every verse in it.
test("a passage is memorized as one card, drilled whole", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.memoryAddPassage("Ps 23:1", "Ps 23:3", new Date().toISOString());
  });
  await page.getByRole("button", { name: "Memorize" }).click();
  // ONE row, named as a range — not three verse rows.
  await expect(page.locator(".card .ref", { hasText: "Ps 23:1–3" })).toHaveCount(1);
  const drill = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return await s.engine.memoryDrill("Ps 23:1", 0);
  });
  expect(drill.label).toBe("Ps 23:1–3");
  expect(drill.verses).toBe(3);
  expect(drill.text).toContain("The LORD is my shepherd");
  expect(drill.text).toContain("He restoreth my soul");
});
