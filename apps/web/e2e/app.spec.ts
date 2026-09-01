import { expect, test, type Page } from "@playwright/test";

// Boot to the reader. There is no first run — the reader IS the first screen,
// and both analysis tiers are on by default, so nothing here has to switch them
// on the way it used to tick the welcome's checkboxes.
async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/// Wait out the whole background pipeline: the analysis tier ready, then the boot
/// trace quiet (every warm/analysis chunk appends one timed entry).
///
/// Only ever call this on a FIRST visit. After a relaunch it waits out exactly the
/// interval a relaunch measurement is about, and destroys it.
async function settleBackground(page: Page): Promise<void> {
  await page.waitForFunction(() => (window as any).__plumbline?.rndState === "ready", null, {
    timeout: 120_000,
  });
  await page.waitForFunction(
    async () => {
      const n = ((await (window as any).__plumbline.rpc.bootTrace()) ?? []).length;
      const prev = (window as any).__settleLen ?? -1;
      (window as any).__settleLen = n;
      return n === prev && n > 10;
    },
    null,
    { timeout: 120_000, polling: 700 },
  );
}

/// Wait until the device actually holds every file its pin names — a different
/// barrier from `settleBackground`, which waits only for what the boot trace
/// narrates. The gap between them is where a test reads "prune deleted a pinned
/// file" for a file that had simply not arrived yet. Expressed against the pin
/// rather than filenames, so each language's ~9 MB corpus is covered by being added.
async function settleDepot(page: Page): Promise<void> {
  await expect
    .poll(
      async () =>
        page.evaluate(async () => {
          const hit = await caches.match(new URL("__depot/pack-pin.json", location.href).href, {
            ignoreVary: true,
          });
          if (!hit) return -1;
          const pin = await hit.json();
          let missing = 0;
          for (const f of pin.files ?? []) {
            if (!f.url) continue;
            const url = new URL(f.url, location.href).href;
            if (!(await caches.match(url, { ignoreVary: true }))) missing++;
          }
          return missing;
        }),
      { timeout: 120_000, message: "the device never finished downloading what its pin names" },
    )
    .toBe(0);
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
  // One stock tag ships. An exact count rather than a floor, because what this
  // guards is stray authoring leftovers in the stock set — a shipped highlight once
  // painted John 3:7 on every fresh install — so a second tag must fail here.
  expect(counts.tags).toBe(1);
});

test("a shared link carries the church, and says so", async ({ page }) => {
  // One QR hands over the Bible and the people who sent it. The welcome used to
  // name them; with it gone the toast is the only sign a link brought a church,
  // which is why it is asserted rather than merely the stored value.
  await page.goto("/?church=Grace+Bible+Church&churchService=600&churchUrl=https%3A%2F%2Fexample.org");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.getByText("Grace Bible Church")).toBeVisible();

  // The address bar is left clean: a bookmark of this is the app, not a link
  // about somebody's church.
  expect(await page.evaluate(() => location.search)).toBe("");

  const church = await page.evaluate(() => (window as any).__plumbline.church);
  // `service` reads off `config.sundayService`, which the link just set — one
  // stored number, not a second copy on the church.
  expect(church).toEqual({ name: "Grace Bible Church", service: 600, url: "https://example.org" });

  // And it survives a relaunch. Waiting on the worker first, not because the app
  // is slow to save — the link's church is flushed, not left to the 300 ms
  // debounce — but because "flushed" means the save is POSTED, and the write
  // itself lands in the worker. Reloading without waiting races the thing this
  // assertion is about, which is how it failed under parallel load while passing
  // alone.
  await page.evaluate(() => (window as any).__plumbline.rpc.flush());
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  const after = await page.evaluate(() => (window as any).__plumbline.church.name);
  expect(after).toBe("Grace Bible Church");
});

test("the deferred machine-tier pack loads after boot", async ({ page }) => {
  await boot(page);
  // Boot ships the core pack only; ensureRnd pulls the analysis pack in. Forced
  // here rather than waiting out the idle timer.
  //
  // The probe must be `morph`, which reads morphology.morphb, the pack's largest
  // file. `conceptNeighbours` answers null whether or not the pack loaded — the one
  // thing a probe may never do.
  const gloss = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.ensureRnd();
    return (await s.engine.morph("John 3:16", 3))?.gloss ?? "";
  });
  expect(gloss, "a machine-tier lookup should answer once the pack is in").not.toBe("");
});

// Deferring the analysis pack kept it off the boot path but also out of the
// session, so every launch put a "one-time download / Load analysis" button in
// front of a reader who had already taken it. It loads itself now.
test("a phone is never asked to approve the analysis pack", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 }); // before goto: deferRnd is read at boot
  await boot(page);
  expect(await page.evaluate(() => (window as any).__plumbline.rndDeferred)).toBe(false);

  await page.waitForFunction(() => (window as any).__plumbline.rndState === "ready", null, {
    timeout: 90_000,
  });
  // Same morphology probe as the test above, and for the same reason.
  const gloss = await page.evaluate(
    async () => (await (window as any).__plumbline.engine.morph("John 3:16", 3))?.gloss ?? "",
  );
  expect(gloss, "the pack really arrived, unasked").not.toBe("");

  await expect(page.getByRole("button", { name: "Load analysis" })).toHaveCount(0);
  await expect(page.getByText(/one-time .* download/)).toHaveCount(0);
});

// A study that genuinely cannot be answered yet must not look frozen: while the
// analysis pack is coming in it has to say so. The null refKey is load-bearing —
// it leaves the read unanswered for real, and an earlier version that asserted
// straight away passed with the guard removed.
test("a study that cannot answer yet says so, and never looks frozen", async ({ page }) => {
  await boot(page);
  // Settle the background load first, or it lands mid-test and flips rndState
  // back to "ready" under us.
  await page.waitForFunction(() => (window as any).__plumbline.rndState === "ready", null, {
    timeout: 90_000,
  });
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.rndState = "loading";
    s.rndDeferred = true;
    s.panel = { kind: "wordStudy", refKey: null, tokenIndex: 0 };
  });
  await page.waitForTimeout(1200);
  await expect(page.locator(".loading")).toBeVisible();

  // A load already under way narrates nothing: no note, no bar, no percentage.
  // Sections appear as their data arrives, so the study stays usable throughout.
  await expect(page.locator(".rnd-note")).toHaveCount(0);
  await expect(page.locator(".rnd-bar")).toHaveCount(0);
  await expect(page.getByText(/Downloading the analysis pack/i)).toHaveCount(0);

  // The one surviving notice is a genuine ask rather than a status: nothing is
  // coming, and spending the download is the reader's decision.
  await page.evaluate(() => ((window as any).__plumbline.rndState = "off"));
  await expect(page.locator(".rnd-note")).toBeVisible();
  await expect(page.getByRole("button", { name: "Load analysis" })).toBeVisible();

  // And nothing promises how long it takes, or that it will not happen again.
  await expect(page.getByText(/takes a few seconds/i)).toHaveCount(0);
  await expect(page.getByText(/every look after this/i)).toHaveCount(0);
});

// The relaunch complaint: close the tab, reopen, click a word, and the engine
// thinks all over again. The settle wait belongs on the FIRST visit only — after
// the reload it would wait out precisely the interval this test measures, which is
// how an earlier version passed green against the live bug. Two assertions, because
// either alone is cheatable:
//   - FAST, against a budget derived from this machine's own settled click rather
//     than a constant;
//   - and the SAME ANSWER as the settled engine gave, because an engine that replies
//     before Strong's and the occurrence index are in returns a thinner study, and
//     "instant but hollow" must not read as warm.
test("after a relaunch, the first word study is already warm", { tag: "@perf" }, async ({ page }) => {
  // KNOWN FAILING, DELIBERATELY. `test.fail()` means "this MUST fail": Playwright
  // errors the run if it ever passes, so the open bug stays visible and the marker
  // clears itself the moment the work lands.
  //
  // The tap is fast now; the answer is thin. A relaunch rebuilds every index from
  // scratch because nothing an engine builds survives the tab, so a study opened
  // mid-warm returns only the sections whose indexes exist and fills in at
  // `warmReady`. The fix is to persist them the way `kjv.jsonl.idxcache` already
  // spares the corpus. Until then this fails on the BLOCK COUNT, not the clock.
  test.fail();
  await boot(page);
  await settleBackground(page);

  // The reference: what a fully warm engine answers, and how quickly.
  const settled = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const t = performance.now();
    const b = await s.engine.wordStudyBlocks("John 3:16", 1, s.gates);
    return { ms: performance.now() - t, blocks: b?.blocks?.length ?? 0 };
  });
  expect(settled.blocks, "the settled engine answers a word study at all").toBeGreaterThan(0);

  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

  // Nothing between text appearing and the click. This is the reader.
  const relaunch = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const t = performance.now();
    const b = await s.engine.wordStudyBlocks("John 3:16", 1, s.gates);
    return { ms: performance.now() - t, blocks: b?.blocks?.length ?? 0 };
  });

  // Derived from the settled click on this same machine, so a loaded CI box moves
  // both sides together; a constant ceiling is part of how the old version stayed
  // green against the live bug.
  const budget = Math.max(250, settled.ms * 5);
  expect(
    relaunch.ms,
    `a relaunch spent ${Math.round(relaunch.ms)}ms answering the reader's first word study ` +
      `(a settled engine answers the same call in ${Math.round(settled.ms)}ms). Nothing was ` +
      `downloaded — this is the engine rebuilding indexes it already built last launch.`,
  ).toBeLessThan(budget);
  expect(
    relaunch.blocks,
    "the relaunched engine answered FAST but with a thinner study than the settled one — " +
      "it replied before its data was in, which is not the same thing as being warm",
  ).toBe(settled.blocks);
});

// The AKJV overlay: modern wording laid over the KJV's own tokens, off until
// asked. It is a reading aid — the text stays the KJV, and nothing outside the
// reader may notice it exists.
test("the AKJV overlay re-words the reader, and only the reader", async ({ page }) => {
  await boot(page);
  await page.waitForFunction(() => (window as any).__plumbline.akjvAvailable === true, null, {
    timeout: 90_000,
  });

  expect(await page.evaluate(() => (window as any).__plumbline.config.akjvOverlay)).toBeFalsy();

  await page.evaluate(() => (window as any).__plumbline.setAkjvOverlay(true));

  // A multi-token run answers from any word inside it: "Verily, verily" is one
  // re-rendering, so tapping either half must explain the same thing.
  const spans = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const out: any[] = [];
    for (const i of [4, 6, 7]) out.push(await s.engine.akjvToken("John 3:3", i));
    return out;
  });
  expect(spans[0]).toEqual({ akjv: "to", kjv: "unto" });
  expect(spans[1]).toEqual({ akjv: "Truly, truly", kjv: "Verily, verily," });
  expect(spans[2]).toEqual(spans[1]);
  expect(await page.evaluate(() => (window as any).__plumbline.engine.akjvToken("John 3:3", 0))).toBeNull();

  // The overlay is applied on the way into layout and nowhere else, so everything
  // that leaves the reader is still the KJV. A modernised word on a memory card or
  // in a hand-off would make this a second translation.
  const kept = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return {
      verse: (await s.engine.verse("John 3:3"))?.body,
      copied: await s.engine.copyText("John 3:3", "verse"),
      drill: (await s.engine.memoryDrill("John 3:3", 0))?.text,
    };
  });
  expect(kept.verse).toContain("Verily, verily");
  expect(kept.verse).not.toContain("Truly");
  expect(kept.copied).toContain("Verily, verily");
  expect(kept.copied).not.toContain("Truly");
  expect(kept.drill).toContain("Verily, verily");

  await page.evaluate(() => (window as any).__plumbline.setAkjvOverlay(false));
  expect(await page.evaluate(() => (window as any).__plumbline.config.akjvOverlay)).toBe(false);
});

// The toggle must change the page, not just the setting: the pane's layout effect
// has its own trigger and does not track the config, so without the layout epoch
// the old words stay on screen until a resize or chapter turn re-lays the chapter.
test("flipping the overlay re-lays the page immediately", async ({ page }) => {
  await boot(page);
  await page.waitForFunction(() => (window as any).__plumbline.akjvAvailable === true, null, {
    timeout: 90_000,
  });
  // What the reader can actually see: the words in the display list.
  const wordsOnScreen = async () =>
    page.evaluate(async () => {
      const s = (window as any).__plumbline;
      const raw = await s.rpc.layout("John", 3, { font: 18, width: 700, lineSpacing: 1.35, versePerLine: false });
      return (raw?.items ?? []).filter((i: any) => i.kind === "word").map((i: any) => i.text).join(" ");
    });

  const before = await wordsOnScreen();
  expect(before).toContain("Verily");

  await page.evaluate(() => (window as any).__plumbline.setAkjvOverlay(true));
  const after = await wordsOnScreen();
  expect(after).toContain("Truly");
  expect(after).not.toContain("Verily");

  await page.evaluate(() => (window as any).__plumbline.setAkjvOverlay(false));
  expect(await wordsOnScreen()).toContain("Verily");

  // The above proves the engine re-lays; this proves the reader asks it to.
  const layouts = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    (window as any).__layoutCalls = 0;
    const real = s.rpc.layout.bind(s.rpc);
    s.rpc.layout = (...a: unknown[]) => {
      (window as any).__layoutCalls++;
      return real(...a);
    };
    return (window as any).__layoutCalls;
  });
  expect(layouts).toBe(0);
  await page.evaluate(() => (window as any).__plumbline.setAkjvOverlay(true));
  await expect
    .poll(() => page.evaluate(() => (window as any).__layoutCalls), { timeout: 10_000 })
    .toBeGreaterThan(0);
});

test("menus open promptly after boot (freeze regression)", { tag: "@perf" }, async ({ page }) => {
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
  await page.getByRole("button", { name: "Study", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Memorize/ }).click();
  await expect(page.getByText("Review due")).toBeVisible();
  await page.getByRole("button", { name: "Study", exact: true }).click();
  await expect(page.getByText("Review due")).toBeHidden();
  // The Visualizations card is the hub's "we're back on Study" landmark.
  await expect(page.getByText("Visualizations")).toBeVisible();
});

test("word study opens from a single click and respects the gates", async ({ page }) => {
  await boot(page);
  const canvas = page.locator("canvas").first();
  const box = (await canvas.boundingBox())!;
  // Walk the first text line until a word hit opens the panel: a single click
  // opens the word study, with no pin/＋link step.
  for (const x of [0.3, 0.35, 0.4, 0.45, 0.5]) {
    await canvas.click({ position: { x: box.width * x, y: 46 } });
    if (await page.locator("aside.panel").isVisible().catch(() => false)) break;
  }
  await expect(page.locator("aside.panel")).toBeVisible();
  // The reader's own-note slot on the usage card: a "Notes" header with a ＋.
  await expect(page.locator("aside.panel").getByText("Notes", { exact: true }).first()).toBeVisible();
  await expect(page.locator("aside.panel").getByText("＋", { exact: true }).first()).toBeVisible();
});

test("live search shows results, and Escape steps back out", async ({ page }) => {
  await boot(page);
  await page.getByLabel("Open search").click();
  await page.getByRole("searchbox").fill("in the beginning");
  await expect(page.locator('[data-surface="search results"]')).toContainText("result");
  // Escape on a field with a query in it empties the field; a second press
  // leaves the screen.
  await page.keyboard.press("Escape");
  await expect(page.getByRole("searchbox")).toHaveValue("");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("searchbox")).toHaveCount(0);
  await expect(page.locator(".pane canvas").first()).toBeVisible();
});

test("settings switch the theme", async ({ page }) => {
  await boot(page);
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByLabel("Theme").selectOption("night");
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
  await page.locator(".bottom-nav").getByRole("button", { name: "Study", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Weaves/ }).click();
  await page.locator(".ex-card", { hasText: /^Browse weaves/ }).click();
  await page.locator("aside.panel button.link").first().click();
  await expect(page.locator(".pane canvas")).toHaveCount(1);
  const panes = await page.evaluate(() => (window as any).__plumbline.panes.length);
  expect(panes).toBe(1);
});

test("phones clamp a restored multi-pane session to one pane", async ({ page }) => {
  // A wide session saves a split; reopening on a phone must restore one pane. The
  // narrow rule guards addPane, but the restore path has to clamp too — a phone
  // once booted into two panes of John 3.
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
  // Every grid comes from the boot-prefetched TOC, so a book's chapter list is on
  // screen immediately. There is no verse step — it laid out a whole chapter just
  // to count verses.
  await boot(page);
  await page.locator(".nav .passage").first().click();
  // The navigator lists one testament and opens on the one the reader is in (a
  // fresh profile is at John 3, i.e. the NT), so Joel needs the OT tab first.
  await page.locator('.dialog [data-testament="ot"]').click();
  await page.getByRole("button", { name: "Joel", exact: true }).click();
  // Joel's three chapters render synchronously — no round trip to the engine.
  await expect(page.locator(".grid.nums button")).toHaveCount(3, { timeout: 1_000 });
  await page.getByRole("button", { name: "3", exact: true }).click();
  await expect(page.locator(".subtitle")).toContainText("Joel 3");
});

test("opening a weave splits to its passages; verse clicks stay responsive (freeze regression)", { tag: "@perf" }, async ({
  page,
}) => {
  await boot(page);
  // Weaves is a door inside the hub: card → Weaves page → Browse raises the
  // library panel. There is no header browse row.
  await page.getByRole("button", { name: "Study", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Weaves/ }).click();
  await page.locator(".ex-card", { hasText: /^Browse weaves/ }).click();
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

  // Clicking a card's verse links used to spiral the layout effect into an
  // effect_update_depth_exceeded freeze (~10s) that killed reactivity.
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
  // Backup sits with the everyday settings, not under Advanced.
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Back up (.zip)" }).click(),
  ]);
  const zipPath = testInfo.outputPath("backup.zip");
  await download.saveAs(zipPath);

  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.userNoteSet("John 3:16", "damaged", "2026-07-25T01:00:00Z");
  });
  // Mark the current document, then wait until the restore's reload has
  // actually replaced it (waitForLoadState resolves against the old page).
  await page.evaluate(() => ((window as any).__preRestore = true));
  await page.locator('input[type="file"]').setInputFiles(zipPath);
  await expect
    // A poll tick landing inside the reload throws "context destroyed" and fails
    // the poll rather than retrying it.
    .poll(
      async () => page.evaluate(() => (window as any).__preRestore ?? null).catch(() => "navigating"),
      {
        timeout: 30_000,
      },
    )
    .toBeNull();
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
  const text = await page.evaluate(
    async () => (await (window as any).__plumbline.engine.userNote("John 3:16"))?.text,
  );
  expect(text).toBe("backup probe");
});

// ── boot resilience ───────────────────────────────────────────────────────────

test("a warm boot never asks the network for the pack or the engine", async ({ page }) => {
  // An offline test cannot tell depot-served from service-worker-served: it passes
  // either way, so it goes green against a boot that still depends on the SW being
  // in its path. The sharp observable is the REQUEST, not the response — a bare
  // fetch the SW answers from its cache still issues a request, while bytes the
  // depot holds are read from storage and no request is made at all. So counting
  // requests on a warm boot separates the two, with the SW left registered.
  //
  // page.on("request") is deliberate: it reports requests made inside the engine
  // worker, where this happens. CDP does not — a dedicated worker is another target.
  await boot(page);
  // Let the background stages finish on the first visit, so their bytes are in the
  // depot before the warm boot being measured. `stage2 load` in the trace is the
  // signal that stage 2 landed, rather than a guessed sleep.
  await expect
    .poll(
      async () =>
        page.evaluate(async () =>
          ((await (window as any).__plumbline?.rpc?.bootTrace()) ?? []).some(([l]: [string]) =>
            l.startsWith("stage2 load"),
          ),
        ),
      { timeout: 90_000 },
    )
    .toBe(true);
  await page.waitForTimeout(1_500); // the shell precache runs at the next idle

  const asked: string[] = [];
  const listener = (r: { url: () => string }) => {
    const u = new URL(r.url());
    // Nothing is exempt, the manifest included: it is the one pack file with no
    // version in its URL, so boot had to ask the network for it and a stalled radio
    // cost the SW's 3.5 s timebox before the app opened. The pin replaced it. The
    // live manifest is still fetched once per session by the reconciler, but off the
    // boot path — which is why this measures only up to the point there is text.
    if (u.pathname.includes("/pack/") || u.pathname.endsWith(".wasm")) asked.push(u.pathname);
  };
  page.on("request", listener);
  let untilText: string[] = [];
  try {
    await page.reload();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 60_000 });
    await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/);
    // Snapshot here: text is on screen, so everything to this point came from
    // storage. What the reconciler fetches afterwards is off the critical path.
    untilText = [...asked];
    // The background stages must be depot hits too, or the reader pays for
    // Strong's and the cross-references again on every launch.
    await expect
      .poll(
        async () =>
          page.evaluate(async () =>
            ((await (window as any).__plumbline?.rpc?.bootTrace()) ?? []).some(([l]: [string]) =>
              l.startsWith("stage2 load"),
            ),
          ),
        { timeout: 90_000 },
      )
      .toBe(true);
  } finally {
    page.off("request", listener);
  }

  expect(
    untilText,
    "a warm boot asked the network for something before it could show text — including the manifest, " +
      "which the pin exists to remove from the boot path",
  ).toEqual([]);

  expect(
    asked,
    "a warm boot re-requested pack bytes or the wasm — the depot is not serving them, so this boot " +
      "depends on the service worker winning a race it does not always win",
  ).toEqual([]);
});

test("read pack files are freed, and the reader can still author afterwards", async ({ page }) => {
  // The WASI shim's File constructor copies what it is handed, so the in-memory
  // home kept a second copy of every pack byte (~37 MB for the corpus cache alone).
  // Files whose single reader has finished are dropped.
  //
  // Eviction is restricted to data/ because `persistUserData` derives deletions by
  // diffing the home against IndexedDB: anything evicted from a user directory is
  // permanently deleted from the reader's storage on their next authoring write.
  // data/kjv-notes.jsonl must survive too, since `load_study` re-reads it on every
  // one of those writes.
  await boot(page);
  await expect
    .poll(
      async () =>
        page.evaluate(async () =>
          ((await (window as any).__plumbline.rpc.bootTrace()) ?? []).some(([l]: [string]) =>
            l.startsWith("home evict after stage 2"),
          ),
        ),
      { timeout: 90_000 },
    )
    .toBe(true);

  const freedKb = await page.evaluate(async () => {
    const trace: [string, number][] = await (window as any).__plumbline.rpc.bootTrace();
    return trace.filter(([l]) => l.startsWith("home evict")).reduce((s, [, kb]) => s + kb, 0);
  });
  // The corpus cache alone is ~36 MB; Strong's and the overlay add ~3.7 MB.
  expect(freedKb, "eviction freed suspiciously little — is it finding the nodes at all?").toBeGreaterThan(
    30_000,
  );

  const after = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    // Authoring makes load_study rebuild all study data from the home — the moment
    // an over-eager eviction shows up.
    await s.engine.userNoteSet("Gen 1:1", "eviction probe", "2026-07-28T00:00:00Z");
    const [weaves, threads, mine, margin] = await Promise.all([
      s.engine.weaves(),
      s.engine.threads(),
      s.engine.userNote("Gen 1:1"),
      // The margin notes come from data/kjv-notes.jsonl, which eviction must never
      // touch. Gen 1:4 has one.
      s.engine.verseNotes("Gen 1:4"),
    ]);
    return {
      weaves: weaves?.weaves?.length ?? 0,
      threads: threads?.threads?.length ?? 0,
      mine: JSON.stringify(mine ?? null),
      margin: JSON.stringify(margin ?? null),
    };
  });
  expect(after.weaves, "the stock weaves vanished — eviction reached a user directory").toBeGreaterThan(20);
  expect(after.threads).toBeGreaterThanOrEqual(1);
  expect(after.mine, "the reader's own note did not survive the study reload").toContain("eviction probe");
  expect(after.margin, "the 1769 margin notes are gone — data/kjv-notes.jsonl was evicted").toContain("Heb.");

  // And the text still pages: the corpus decodes out of wasm memory, not the node
  // that was dropped.
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Rev", 22));
  await expect(page.locator(".subtitle")).toHaveText(/Revelation 22/, { timeout: 30_000 });
});

test("checking for an update cannot poison the cached shell", async ({ page }) => {
  // The bug: the update check fetched index.html as data and the SW's network-first
  // branch cached every ok response, so merely asking whether an update existed
  // wrote a newer shell whose /assets/* were absent, and the next offline launch
  // white-screened. Two rules now hold — no-store responses are never cached, and
  // index.html is cached only for a real navigation, recognised by PATHNAME (keyed
  // on the full URL, `?as-data-probe` sailed past).
  await boot(page);
  // The refusals only mean anything if the worker is in the request path at all.
  await expect
    .poll(async () => page.evaluate(() => !!navigator.serviceWorker.controller), { timeout: 30_000 })
    .toBe(true);

  const { controlMs, noStoreCached, dataDocCached } = await page.evaluate(async () => {
    const cache = await caches.open("plumbline-v1");
    const seen = (u: string) => cache.match(u, { ignoreVary: true }).then((h) => !!h);
    /** Wait up to `ms` for `u` to appear; the wait it actually took, or null. */
    const settle = async (u: string, ms: number): Promise<number | null> => {
      const t0 = performance.now();
      for (;;) {
        if (await seen(u)) return performance.now() - t0;
        if (performance.now() - t0 >= ms) return null;
        await new Promise((r) => setTimeout(r, 25));
      }
    };

    // Do not read the cache straight after the fetch: the SW's cache.put is
    // fire-and-forget, so an immediate cache.match measures the race and not the
    // rule — that is how this passed on chromium against the live bug. The window is
    // derived from this machine, not a constant: a response the worker IS supposed
    // to cache is timed on its way in, then a refused one gets ten times that long
    // to show up anyway.
    const control = new URL("icon.svg?control-probe", location.href).href;
    await fetch(control).catch(() => {});
    const controlMs = await settle(control, 20_000);
    const grace = Math.max(1_000, (controlMs ?? 1_000) * 10);

    // A no-store request for something not otherwise stored.
    const probe = new URL("icon.svg?no-store-probe", location.href).href;
    await fetch(probe, { cache: "no-store" }).catch(() => {});
    // index.html asked for as data, the exact shape the update check used.
    const asData = new URL("index.html?as-data-probe", location.href).href;
    await fetch(asData).catch(() => {});
    return {
      controlMs,
      noStoreCached: (await settle(probe, grace)) !== null,
      dataDocCached: (await settle(asData, grace)) !== null,
    };
  });
  expect(
    controlMs,
    "the control response never reached the cache, so this worker cached nothing at all and the two " +
      "refusals below would pass for the wrong reason",
  ).not.toBeNull();
  expect(noStoreCached, "a no-store response was cached — the request asked not to be answered from cache").toBe(
    false,
  );
  expect(dataDocCached, "index.html fetched as data was cached — this is the white-screen vector").toBe(false);
});

test("a shared deep link does not strand its own copy of the shell", async ({ page }) => {
  // Navigations were cached under the URL requested, so every distinct deep link
  // accumulated its own index.html that the sweep never touched (un-versioned
  // entries are exempt). Offline, that stale copy named a bundle since pruned: a
  // white screen for shared links only, while the plain app worked.
  //
  // Boot first. On a first visit the SW is not controlling the page, so the
  // navigation never reaches its fetch handler and no entry is written whatever the
  // key logic does — the test would prove nothing.
  await boot(page);
  await expect
    .poll(async () => page.evaluate(() => !!navigator.serviceWorker.controller), { timeout: 30_000 })
    .toBe(true);

  await page.goto("/?at=Ps+23:1");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await page.waitForTimeout(1_500);

  const queried = await page.evaluate(async () => {
    const cache = await caches.open("plumbline-v1");
    return (await cache.keys()).map((r) => r.url).filter((u) => u.includes("at=") || u.includes("church="));
  });
  expect(queried, "a deep link stored its own shell copy under its query string").toEqual([]);
});

test("the whole shell is stored after one visit, not just what this page loaded", async ({ page }) => {
  // A precache driven by this page's resource timeline stored whatever happened to
  // load, so a chunk imported lazily for a screen the reader had not opened was
  // simply missing offline. The build emits the shell's exact file list instead,
  // and this asserts the depot holds all of it.
  await boot(page);
  await page.waitForTimeout(1_500); // the precache runs at the first idle

  const { missing, total } = await page.evaluate(async () => {
    const manifest = await (await fetch("shell-manifest.json")).json();
    const cache = await caches.open("plumbline-v1");
    const missing: string[] = [];
    for (const f of manifest.files) {
      if (!(await cache.match(new URL(f, location.href).href, { ignoreVary: true }))) missing.push(f);
    }
    return { missing, total: manifest.files.length };
  });
  expect(total, "the shell manifest should list the bundles, the fonts and the icons").toBeGreaterThan(8);
  expect(missing, "these shell files are not on the device — an offline launch would white-screen").toEqual(
    [],
  );
});

test("the engine worker measures with the real reader font, not a fallback", async ({ page }) => {
  // Layout is measured in the worker over an OffscreenCanvas, so it needs the real
  // EB Garamond in its own FontFaceSet: with a fallback face it measures different
  // advance widths than the main thread paints, and lines wrap where they are not
  // drawn. That failure is silent by design, which is why it needs a test. Also
  // pins that the worker's FontFace path accepts woff2 at all.
  await boot(page);
  const faces = await page.evaluate(async () => {
    const trace: [string, number][] = await (window as any).__plumbline.rpc.bootTrace();
    return trace.find(([l]) => l === "worker font faces")?.[1];
  });
  expect(faces, "the worker must load BOTH reader faces (roman + italic)").toBe(2);
});

test("a first visit never parses the corpus — the pack ships the cache", async ({ page }) => {
  // Every test starts on empty storage, so this is a first visit. Parsing ~19 MB of
  // JSONL here cost 8.4 s on a real phone; the pack ships a prebuilt idxcache
  // instead (hydrate `web-cache`, stamped mtime 0 as the browser WASI shim reports).
  // If it stops shipping or stops validating, the label flips and this fails.
  await boot(page);
  const label = await page.evaluate(async () => {
    const trace: [string, number][] = await (window as any).__plumbline.rpc.bootTrace();
    return trace.find(([stage]) => stage.startsWith("engine open"))?.[0];
  });
  expect(label).toBe("engine open (idxcache fast path)");
});

test("background loading never starves the reader (worker-thread scheduling)", { tag: "@perf" }, async ({ page }) => {
  // Stage-2 data and the analytics warm-up run on the one thread that also answers
  // layout, so they must stay chunked with yields. Unchunked, a pane re-layout
  // queued behind seconds of work and left the reader half-width after a split.
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
  // Self-calibrating: a queued layout may wait out one background chunk, never the
  // whole sequence, so the budget follows this machine's own chunk cost and a slow
  // CI box moves both sides together. A fixed millisecond ceiling does NOT work
  // here — 1500 ms sat comfortably above a deliberately un-chunked warm.
  expect(worst).toBeLessThan(Math.max(400, chunk * 2.5));
});

// The companion to the test above, and the reason it needed one: that test derives
// its budget from the WORST chunk, so a phase that is not sliced at all raises its
// own ceiling and can never fail it (a 54,859 ms chunk on a phone set that budget
// to 137 s). This one derives from the MEDIAN instead — one chunk many times the
// typical one is not a slice but a block, and while it runs the thread answers no
// layout, no tap and no word study. The floor keeps it from flaking on a GC spike.
// It is the only guard left against an unsliced background phase, and catches the
// next one on whatever hardware runs it, without needing a slow device.
test("no single background chunk may monopolise the engine worker", { tag: "@perf" }, async ({ page }) => {
  await boot(page);
  await settleBackground(page);

  const { worst, worstLabel, median, count } = await page.evaluate(async () => {
    const trace: [string, number][] = await (window as any).__plumbline.rpc.bootTrace();
    // Only the stages that claim to be sliced. The stage-2 Strong's parse is one
    // synchronous block by construction and is a separate question.
    const chunks = trace.filter(
      ([l]) => l.startsWith("warm step") || l.startsWith("rnd load step"),
    );
    const sorted = chunks.map(([, ms]) => ms).sort((a, b) => a - b);
    let worst = -1;
    let worstLabel = "";
    for (const [l, ms] of chunks) if (ms > worst) ((worst = ms), (worstLabel = l));
    return {
      worst,
      worstLabel,
      median: sorted.length ? sorted[Math.floor(sorted.length / 2)] : 0,
      count: chunks.length,
    };
  });

  expect(count, "no sliced background stages ran at all — this test measured nothing").toBeGreaterThan(5);
  const budget = Math.max(400, median * 6);
  expect(
    worst,
    `"${worstLabel}" held the engine worker for ${worst}ms against a ${median}ms median chunk ` +
      `(${count} chunks). A warm phase that is not sliced blocks every layout and tap RPC ` +
      `queued behind it for its whole duration.`,
  ).toBeLessThan(budget);
});

test("the reader scrolls natively, and the pane follows both ways", async ({ page }) => {
  // Scrolling is the browser's (a spacer sized to the chapter, canvas sticky on
  // top), which is where momentum and fling come from on a phone. Guard the wiring:
  // real scroll range, and scrollTop <-> pane.scrollY tracking both ways.
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
  // The nav strip and header change the instant the reader taps, but the display
  // list arrives from the worker — holding the previous chapter on the canvas
  // meanwhile put John's text under a header reading Acts. Slowing the layout makes
  // that in-between state observable; the verse geometry is what the canvas paints.
  await boot(page);
  // The first chapter must be on screen before navigating away, or the assertion
  // below passes vacuously against the bug.
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
  // …and the header shows the book's name, never its OSIS id ("Rev 7").
  await expect(page.locator(".subtitle")).toHaveText("Revelation 7", { timeout: 30_000 });
});

test("Settings can make the app completely offline, and says when it is", async ({ page }) => {
  // Verifies every pack file is really in the offline cache and fetches whatever is
  // not: a failed download or an eviction otherwise goes unnoticed until the reader
  // has no connection.
  await boot(page);
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Settings" }).click();
  // Offline lives behind the Advanced disclosure.
  await page.locator('[data-surface="settings"] details.advanced > summary').click();
  const download = page.getByRole("button", { name: "Download everything" });
  if (await download.isVisible().catch(() => false)) await download.click();
  await expect(page.getByText("Everything is on this device")).toBeVisible({ timeout: 120_000 });

  // Not just a label: every file the app actually reads must be on the device.
  // Checked against the URLs the app itself uses — read from the manifest and keyed
  // the way the loader keys them (per-file content hash). A test that rebuilds the
  // URL scheme independently only asserts that two copies of the scheme agree.
  const { missing, rawJsonlShipped } = await page.evaluate(async () => {
    const manifest = await (await fetch("pack/manifest.json")).json();
    const cache = await caches.open("plumbline-v1");
    const key = (f: { path: string; hash: string }) => `pack/${f.path}.gz?h=${f.hash}`;
    const missing: string[] = [];
    for (const f of manifest.files) {
      if (!(await cache.match(new URL(key(f), location.href).href, { ignoreVary: true }))) {
        missing.push(f.path);
      }
    }
    return {
      missing,
      rawJsonlShipped: manifest.files.some((f: { path: string }) => f.path === "data/kjv.jsonl"),
    };
  });
  expect(missing, "these pack files are not on the device").toEqual([]);
  // The raw JSONL left the pack: the corpus cache supersedes it, and with the
  // JSONL in the home the engine would parse 19 MB and write a 37 MB cache back.
  expect(rawJsonlShipped, "data/kjv.jsonl is back in the pack").toBe(false);
});

test("the share QR encodes the church, not just the app", async ({ page }) => {
  // One scan hands over both. The QR is generated at render time, so setting a
  // church must change what it encodes — a longer payload needs a bigger symbol.
  await boot(page);
  // Desktop puts the roles bar in the header, phones at the bottom — the
  // navigation landmark covers both.
  await page.getByRole("navigation").getByRole("button", { name: "Share", exact: true }).click();
  const card = page.locator('[data-surface="share app"]');
  await expect(card).toBeVisible();
  const modulesFor = async () =>
    card.locator("svg").getAttribute("viewBox").then((v) => Number(v!.split(" ")[2]));
  const plain = await modulesFor();
  // The card shows the host, never the full link — with a church attached the URL
  // runs off a phone screen.
  await expect(card).toContainText("plumblinebible.org");

  await page.evaluate(() =>
    (window as any).__plumbline.setChurch({
      name: "Grace Bible Church",
      service: 600,
      url: "https://example.org",
    }),
  );
  await expect(card).toContainText("Grace Bible Church");
  expect(await modulesFor()).toBeGreaterThan(plain);
});

// Sharing a passage is a QR carrying the passage, not a share sheet carrying a wall
// of text: what the person in front of you scans must land in the reader at the verse.
test("Present shares the passage as a QR whose link opens at the first verse", async ({ page }) => {
  await boot(page);
  // Through the Share screen, which is where Present is raised now that the
  // welcome's "Sharing the gospel" path is gone.
  await page.getByRole("navigation").getByRole("button", { name: "Share", exact: true }).click();
  await page.getByRole("button", { name: "Present the Gospel" }).click();
  await expect(page.locator(".present .title")).toContainText("Romans Road");

  // Record what the copy button hands over, without needing clipboard perms.
  await page.evaluate(() => {
    (window as any).__copied = [];
    navigator.clipboard.writeText = async (t: string) => void (window as any).__copied.push(t);
  });

  // Present's own Share, not the header's.
  await page.locator(".present .sharebtn").click();
  await expect(page.locator(".sharesheet svg")).toBeVisible();
  await page.getByRole("button", { name: "Copy the passages" }).click();

  const copied: string = await page.evaluate(() => (window as any).__copied[0]);
  // The link carries the thread's first verse, url-encoded ("Rom 3:23").
  expect(copied).toMatch(/[?&]at=Rom\+3%3A23/);
  expect(copied).toContain("For all have sinned");
});

// The receiving half of that QR: the link must actually land on the verse.
test("a shared passage link opens the reader at its verse", async ({ page }) => {
  await page.goto("/?at=Ps+23%3A1");
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

// Every versioned URL is content-addressed, so an update adds an entry beside the
// old one; with nothing removing the old, three data updates stranded three whole
// ~12 MB packs on the device.
test("updating sweeps the versions this build no longer uses", async ({ page }) => {
  await boot(page);
  // The pin names every file in the manifest and is written the moment the engine
  // opens, long before stage 2, the analysis pack and the other languages' corpora
  // have finished downloading. Without both settles the last assertion here ("every
  // file the pin names survived the sweep") reports prune for a file that had simply
  // not arrived yet.
  await settleBackground(page);
  await settleDepot(page);
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
    // Entries that must survive: the un-versioned shell.
    await put(location.origin + "/index.html");
    void s;
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
  // ...and nothing else was collateral: the shell and every file the pin names must
  // survive, or the next launch is broken or offline-dead. The keep-set is the pin
  // plus the shell manifest, which is what lets per-file hashes work and reclaims a
  // file dropped from the pack entirely.
  expect(after).toContain("/index.html");
  const pinned = await page.evaluate(async () => {
    const hit = await caches.match(new URL("__depot/pack-pin.json", location.href).href, {
      ignoreVary: true,
    });
    const pin = hit ? await hit.json() : null;
    // Only the files the pin names. An `optional` file the reader never asked for
    // is listed without a url — the pin saying the device does not have it — and
    // prune is right not to keep it.
    return (pin?.files ?? [])
      .filter((f: { url?: string }) => f.url)
      .map((f: { url: string }) => "/" + f.url);
  });
  // A floor, not a count: this only asserts a pin exists and names the whole pack —
  // the loop below is the real check. Kept well under the true file count so that
  // dropping an artifact does not fail here for no reason.
  expect(pinned.length, "there should be a pin naming the pack after a boot").toBeGreaterThan(30);
  for (const u of pinned) expect(after, `prune deleted a pinned pack file: ${u}`).toContain(u);
  // The bundle this page is actually running must still be cached.
  const running = await page.evaluate(
    () => document.querySelector<HTMLScriptElement>('script[type="module"][src*="/assets/"]')!.src,
  );
  expect(after).toContain(new URL(running).pathname);
});

// A deploy landed while this session stayed open. Driven through the real checker
// with a stubbed manifest, so it exercises the build comparison, not just the flag.
test("a new deploy offers an update, and only when the build really changed", async ({ page }) => {
  await boot(page);
  const realFetch = "__realFetch";
  await page.evaluate((k) => {
    (window as any)[k] = window.fetch.bind(window);
  }, realFetch);

  // Same build deployed → no toast; a checker that always fires nags every reader on
  // every resume. The signal is the shell manifest's `buildId`, not a regex over
  // index.html — scraping the document made the SW cache a newer shell whose bundles
  // did not exist yet.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const live = await (await (window as any).__realFetch("shell-manifest.json")).json();
    window.fetch = async () => new Response(JSON.stringify(live));
    await s.checkForUpdate(true);
  });
  await expect(page.locator(".toast.update")).toHaveCount(0);

  // A different build id → the offer appears.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    window.fetch = async () => new Response(JSON.stringify({ buildId: "NEWBUILD", files: [] }));
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
// outright. Staying on John proves little on its own, since the link dispatcher
// discards an unparseable ref anyway — the signal is the address bar, because the
// shell strips the query only once it has consumed something from it.
test("a bogus at= parameter is rejected, not merely survived", async ({ page }) => {
  await page.goto("/?at=javascript%3Aalert(1)");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });

  const book = await page.evaluate(() => (window as any).__plumbline.panes[0].book);
  expect(book).toBe("John"); // the default landing, untouched
  expect(await page.evaluate(() => location.search)).toContain("at=");
});

test("the phone top bar stays on one row, search behind a glass", async ({ page }) => {
  // Welcome + Church + Share + Search + ≡ wrapped onto a second row on a phone.
  // Search is an icon and a door to its own screen, so it never takes the row.
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  await page.evaluate(() =>
    (window as any).__plumbline.setChurch({ name: "Grace Bible Church", info: "", url: "https://example.org" }),
  );
  // "One row" means the visible children share a row as each other, compared among
  // themselves: measuring against the header's own top also asserts a bar height,
  // and goes red the moment the bar is deliberately made taller. display:none
  // children are skipped — their all-zero rects would drag the spread to the full
  // bar height whatever the layout did.
  const oneRow = () =>
    page.locator("header").evaluate((h) => {
      const tops = [...h.children]
        .filter((c) => c.getBoundingClientRect().height > 0)
        .map((c) => c.getBoundingClientRect().top);
      return tops.length > 0 && Math.max(...tops) - Math.min(...tops) < 24;
    });
  await expect.poll(oneRow).toBe(true);

  // The bar carries no field: the glass is a door to the search screen. What matters
  // here is that it is reachable on a phone and leaves the one-row promise intact.
  await expect(page.locator("header").getByRole("searchbox")).toHaveCount(0);
  await page.getByLabel("Open search").click();
  await expect(page.getByRole("searchbox")).toBeFocused();
  await page.getByRole("searchbox").fill("in the beginning");
  await expect(page.locator('[data-surface="search results"]')).toContainText("result");
});

// The Check button once read the engine through `session.engine`, the console/e2e
// proxy, which returns a promise — so `score.accuracy` was undefined and every
// check reported "0% recalled", even a verbatim copy. Drives the real UI, so a
// regression in either the wiring or the scoring fails here.
test("checking a typed recall scores it (a perfect copy is 100%)", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.memoryAdd("John 3:16", new Date().toISOString());
  });
  await page.getByRole("button", { name: "Study", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Memorize/ }).click();
  await page.getByRole("button", { name: "Review due", exact: false }).click();
  await page.getByRole("button", { name: "Type it" }).click();

  const drilled = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return (await s.engine.memoryDrill("John 3:16", 0))?.text as string;
  });
  expect(drilled).toContain("For God so loved");
  await page.locator("textarea").fill(drilled);
  await page.getByRole("button", { name: "Check" }).click();
  await expect(page.locator(".accuracy")).toHaveText("100% recalled");

  // A second check must rescore, not leave the first score on screen. Pinned to the
  // exact figure because `not.toHaveText("100% recalled")` also passes when
  // `.accuracy` is missing, greeting a silently cleared score as a success. This
  // answer shares one of John 3:16's 25 words: 4%.
  await page.locator("textarea").fill("nothing like the verse at all");
  await page.getByRole("button", { name: "Check" }).click();
  await expect(page.locator(".accuracy")).toHaveText("4% recalled");
});

// A reader who pauses to think must not lose their work. `nowStamp()` is
// second-granularity and lands in the read-through cache key, so the due-list read
// minted a fresh key every second, fell back to [], and re-ran the reset effect —
// clearing the textarea and dropping the mode back to "First letters" about once a
// second. The 2.5 s dwell is not a performance budget but a span that must straddle
// the one-second boundary that churn ran on.
test("a typed recall survives a pause and a background study refresh", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.memoryAdd("John 3:16", new Date().toISOString());
  });
  await page.getByRole("button", { name: "Study", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Memorize/ }).click();
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

// Through the picker, not the engine: commit() read `start`/`throughRef` after
// close() had nulled the state they derive from, so every attempt toasted "null or
// invalid argument" with no card written. The test below, which seeds via
// memoryAddPassage directly, sails straight past that.
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

// A whole section as one card: the hub lists one row labelled with the range, and
// the drill covers every verse in it.
test("a passage is memorized as one card, drilled whole", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.engine.memoryAddPassage("Ps 23:1", "Ps 23:3", new Date().toISOString());
  });
  await page.getByRole("button", { name: "Study", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Memorize/ }).click();
  // One row, named as a range — not three verse rows.
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
