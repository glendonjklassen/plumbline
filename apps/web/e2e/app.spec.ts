import { expect, test, type Page } from "@playwright/test";

// Boot the app and wait for the reader. On a fresh profile the first-run
// chooser OWNS the screen straight off the loader (the reader mounts after a
// path is chosen — 2026-07-26); take the established path. The title check
// also pins the product branding — index.html, the manifest and the shell
// header must agree.
//
// The tier checkboxes are TICKED here (2026-07-28). The analysis tiers became
// opt-in that day — they used to be on unless switched off — so an untouched
// first run now lands a reader with the text and nothing else, and every test
// below that is about the analysis pack (it arrives without approval, a relaunch
// is already warm, no chunk monopolises the worker, updates sweep old versions)
// would otherwise sit waiting for a download that is correctly never requested.
// Ticking them keeps those tests measuring what they were written to measure: a
// reader who HAS the analysis on. `optOutOfAnalysis` is the other side of it.
async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  const established = page.getByRole("button", { name: "Established believer" });
  // Either the chooser (fresh profile) or the reader canvas (returning) —
  // the canvas, not .subtitle, because phones hide the subtitle.
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    for (const box of await page.locator(".dialog label.card input[type=checkbox]").all())
      if (!(await box.isChecked())) await box.check();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/// Wait out the WHOLE background pipeline: the analysis tier ready, then the boot
/// trace stops growing (every warm/analysis chunk appends one timed entry, and
/// the trace goes quiet only when the last one has run).
///
/// Where this is called is the entire point. On the FIRST visit it is legitimate
/// setup — get the device into the state a returning reader is in. Called after a
/// RELAUNCH it destroys the measurement, because the interval it waits out is the
/// interval every relaunch complaint has been about.
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
  // ONE stock tag ships (95ff71b): "False teaching", two verses, no notes — the
  // only example a reader ever sees of what a tag is for. It was zero until then,
  // and the exact count is asserted rather than a floor, because the thing this
  // line has always guarded is stray AUTHORING LEFTOVERS in the stock set: a
  // shipped amber highlight once painted John 3:7 on every fresh install. A
  // second stock tag arriving unnoticed is the failure worth catching.
  expect(counts.tags).toBe(1);
});

test("first-run: the welcome owns the boot screen, with no reader behind it", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "New believer" })).toBeVisible({ timeout: 90_000 });
  // John 3 used to paint underneath and then get asked a question — the
  // reader must not mount until a path is chosen (feedback 2026-07-26).
  await expect(page.locator(".pane canvas")).toHaveCount(0);
  await expect(page.locator("header .search")).toHaveCount(0);
});

test("first-run: a new believer's welcome reference opens beside John", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New believer" }).click({ timeout: 90_000 });
  await expect(page.getByText("I'm so glad you've put your faith in Jesus")).toBeVisible();
  // "Psalms", not "Psalm": the chip's label is DERIVED now — the book's name
  // from the canon plus the catalogue's `ref.range` template — so that it
  // localizes and so that it cannot disagree with what the app calls the book
  // everywhere else. It was hand-typed as "Psalm 12:6–7" before (2026-08-03).
  await page.getByRole("button", { name: "Psalms 12:6–7" }).click();
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
  await expect(page.getByText(/links and QR codes you share contain your church/)).toBeVisible();
  await page.getByPlaceholder("Church name").fill("Grace Bible Church");
  await page.getByPlaceholder(/When and where/).fill("Sundays 10am, 12 Long Street");
  await page.getByRole("button", { name: "Open the presentation screen" }).click();
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
  // Boot ships the core pack only (TODO #28); ensureRnd pulls the analysis pack
  // in and re-warms. Force it (instead of waiting out the idle timer) and check
  // a machine-tier lookup lights up.
  //
  // The probe is MORPHOLOGY. It used to be `conceptNeighbours`, which read the
  // concept embedding — dropped from the pack 2026-07-30 — so it now answers
  // null whether or not the pack loaded, which is the one thing a probe may
  // never do. `morph` reads `morphology.morphb`, the pack's largest remaining
  // file, and John 3:16 token 3 is the token the FFI tests use for it.
  const gloss = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    await s.ensureRnd();
    return (await s.engine.morph("John 3:16", 3))?.gloss ?? "";
  });
  expect(gloss, "a machine-tier lookup should answer once the pack is in").not.toBe("");
});

// A PHONE must never be asked to approve the analysis pack. Deferring it keeps
// it off the boot path; it was also keeping it out of the session entirely, so
// every launch put a "one-time download / Load analysis" button in front of a
// reader who had already taken that download — and, on the same screen, a
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
  // Same morphology probe as the test above, and for the same reason.
  const gloss = await page.evaluate(
    async () => (await (window as any).__plumbline.engine.morph("John 3:16", 3))?.gloss ?? "",
  );
  expect(gloss, "the pack really arrived, unasked").not.toBe("");

  // And the offer never appeared.
  await expect(page.getByRole("button", { name: "Load analysis" })).toHaveCount(0);
  await expect(page.getByText(/one-time .* download/)).toHaveCount(0);
});

// This test used to be "a loading study explains itself once, not twice", and it
// balanced two notices against each other: the analysis pack's progress line, and
// underneath it a "the first one takes a few seconds… every look after this is
// instant" note.
//
// That second note is gone (2026-07-28) and its half of this test with it. It was
// an apology for a bug, and an inaccurate one: "every look after this" lasted
// until the tab closed, and the next launch rebuilt the same indexes and said it
// again — so a reader who had used the app for days kept being told it was their
// first time. The wait itself is gone too, now that nothing builds an index
// inside a reader's request.
//
// What is still worth pinning is the half that was never about the apology: a
// study that genuinely cannot be answered yet must not look frozen, and while the
// analysis pack is coming in it must say so. A null refKey leaves the read
// unanswered for real, which is the only way this means anything — an earlier
// version asserted straight away and passed with the guard removed.
test("a study that cannot answer yet says so, and never looks frozen", async ({ page }) => {
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
  await page.waitForTimeout(1200);
  await expect(page.locator(".loading")).toBeVisible();

  // A load ALREADY UNDER WAY narrates nothing: no note, no bar, no percentage.
  // Those existed to account for a wait the reader was made to sit through, back
  // when the analysis load blocked the one thread that answers taps. It does not
  // block anything now — sections appear as their data arrives — and a progress
  // bar over a study that is already usable invents a problem (2026-07-28).
  await expect(page.locator(".rnd-note")).toHaveCount(0);
  await expect(page.locator(".rnd-bar")).toHaveCount(0);
  await expect(page.getByText(/Downloading the analysis pack/i)).toHaveCount(0);

  // What survives is the one case that is a genuine ASK rather than a status:
  // nothing is coming, and spending the download is the reader's decision.
  await page.evaluate(() => ((window as any).__plumbline.rndState = "off"));
  await expect(page.locator(".rnd-note")).toBeVisible();
  await expect(page.getByRole("button", { name: "Load analysis" })).toBeVisible();

  // And nothing promises the reader anything about how long it will take, or
  // that it will not happen again. Both were untrue for a year.
  await expect(page.getByText(/takes a few seconds/i)).toHaveCount(0);
  await expect(page.getByText(/every look after this/i)).toHaveCount(0);
});

// THE relaunch complaint: wipe data, open, click a word — it thinks for a
// second, fine. Close the tab, reopen, click a word — it thinks all over again.
// Every launch, forever. Reported 2026-07-27, and AGAIN on 2026-07-28 with a boot
// trace off the device, because the fix worked and this test did not.
//
// WHAT THIS TEST USED TO DO, and why it is worth spelling out. It reloaded, then
// waited for `rndState === "ready"` and for the boot trace to stop growing —
// up to 180 s of waiting — and only then timed a click, against a budget of
// 250 ms. `rndReady` is posted at engine.worker.ts:227, after `await
// warmChunked()`; so that first wait alone guaranteed a fully settled engine.
// There is no engine state in which that measurement is slow. It waited out
// precisely the interval the reader was complaining about and then reported that
// the far side of it was fast. It passed, green, against the live bug.
//
// That is the third test in this suite to pass against the bug it was written
// for (see CLAUDE.md: page.route() bypassing service workers; a fixed ms ceiling
// a whole un-chunked warm fit inside). The pattern in all three is the same —
// the instrument was chosen after the mechanism, so it could only agree.
//
// The settle wait now happens on the FIRST visit, where it is honest setup:
// visit one pays for everything, visit two clicks a word the moment there is
// text. Two assertions, because either alone is cheatable:
//   - FAST, against a budget derived from this machine's own settled click
//     rather than a constant;
//   - and the SAME ANSWER as the settled engine gave, because an engine that
//     replies before Strong's / the occurrence index / the concept model are in
//     returns a thinner study, and "instant but hollow" must not read as warm.
test("after a relaunch, the first word study is already warm", { tag: "@perf" }, async ({ page }) => {
  // KNOWN FAILING, DELIBERATELY, as of 2026-07-28. Read this before "fixing" it.
  //
  // `test.fail()` means "this MUST fail" — Playwright reports it as an expected
  // failure, and errors the run if it ever PASSES. So the open bug stays visible
  // in every run and this marker clears itself the moment the work lands, which
  // is the opposite of skipping it.
  //
  // WHAT IS STILL BROKEN, and it is no longer the same thing it was this morning.
  // The tap is FAST now; the answer is THIN. Measured on this machine:
  //
  //     settled                        11 ms · 64 blocks
  //     relaunch, tapped immediately   10 ms · 12 blocks   <- fails here
  //     relaunch, after warm finishes    9 ms · 64 blocks
  //
  // The engine no longer builds an index inside a reader's tap (it froze a phone
  // for 21,966 ms doing exactly that), so a study opened mid-warm returns only
  // the sections whose indexes exist, and fills in when `warmReady` lands. Better
  // than a frozen app by any measure, and still not "already warm": a relaunch
  // rebuilds every one of those indexes from scratch because nothing an engine
  // builds survives the tab.
  //
  // The fix is to stop rebuilding them — build once, keep the result, load it
  // back — the way `kjv.jsonl.idxcache` already spares the corpus. Until that
  // lands this test fails on the BLOCK COUNT, not the clock.
  //
  // One thing already fixed that this test does NOT measure, with its own guard:
  // the tap-builds-nothing rule, covered by
  // `a_tap_never_builds_indexes_under_a_sliced_warm` in plumbline-ffi. (There
  // were two. The other was warm phase 7, the "verses like this" SIF model — a
  // single unsliced 54,859 ms block on a phone — and both that phase and its
  // Rust slicing guard went with the feature on 2026-07-30.)
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

  // NOTHING between text appearing and the click. This is the reader.
  const relaunch = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const t = performance.now();
    const b = await s.engine.wordStudyBlocks("John 3:16", 1, s.gates);
    return { ms: performance.now() - t, blocks: b?.blocks?.length ?? 0 };
  });

  // Derived from the settled click on this same machine, so a loaded CI box
  // moves both sides together. A constant is the wrong instrument here and was
  // part of how the old version stayed green.
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

// The plain-English overlay (2026-07-27): the AKJV's wording laid over the KJV's
// own tokens, off until asked. The text stays the KJV — this is a reading aid,
// and the rest of the app must not notice it exists.
test("the AKJV overlay re-words the reader, and only the reader", async ({ page }) => {
  await boot(page);
  await page.waitForFunction(() => (window as any).__plumbline.akjvAvailable === true, null, {
    timeout: 90_000,
  });

  // Off by default: the reader is looking at the KJV.
  expect(await page.evaluate(() => (window as any).__plumbline.config.akjvOverlay)).toBeFalsy();

  await page.evaluate(() => (window as any).__plumbline.setAkjvOverlay(true));

  // A multi-token run answers from ANY word inside it — "Verily, verily" is one
  // re-rendering, and tapping either half must explain the same thing.
  const spans = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const out: any[] = [];
    for (const i of [4, 6, 7]) out.push(await s.engine.akjvToken("John 3:3", i));
    return out;
  });
  expect(spans[0]).toEqual({ akjv: "to", kjv: "unto" });
  expect(spans[1]).toEqual({ akjv: "Truly, truly", kjv: "Verily, verily," });
  expect(spans[2]).toEqual(spans[1]);
  // A word the AKJV left alone has no answer at all.
  expect(await page.evaluate(() => (window as any).__plumbline.engine.akjvToken("John 3:3", 0))).toBeNull();

  // INTEGRITY. The overlay is applied on the way into the layout and nowhere
  // else, so everything that leaves the reader is still the KJV. A modernised
  // word on a memory card or in a hand-off would make this a second
  // translation, whatever the About page says.
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

  // And it turns back off.
  await page.evaluate(() => (window as any).__plumbline.setAkjvOverlay(false));
  expect(await page.evaluate(() => (window as any).__plumbline.config.akjvOverlay)).toBe(false);
});

// The toggle must change the PAGE, not just the setting. The reader's layout
// has its own trigger and does not track the config, so the first version
// flipped the flag and left the old words on screen until something else
// happened to re-lay the chapter (feedback 2026-07-27, "isn't live").
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

  // The above proves the ENGINE re-lays. This proves the READER asks it to:
  // the pane's layout effect has its own trigger and does not track the
  // setting, so without the layout epoch the toggle changes nothing on screen
  // until a resize or a chapter turn happens to re-lay it.
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
  // A weave open must navigate the single pane, not split it. Study is a
  // bottom-bar destination since the menu rationalization.
  await page.locator(".bottom-nav").getByRole("button", { name: "Study", exact: true }).click();
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
  // Weaves lives inside Explore now (Android parity — no header browse row).
  await page.getByRole("button", { name: "Study", exact: true }).click();
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
  // Backup folded into Advanced with the menu rationalization.
  await page.locator('[data-surface="settings"] details.advanced > summary').click();
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
//
// "boots offline after ONE visit" used to live here. It moved to
// network.spec.ts (2026-07-30) because its offline had to become a DEAD ORIGIN:
// context.setOffline(true) makes WebKit stop consulting the service worker
// altogether, so the one engine where the offline promise is hardest to keep was
// the one engine that could not check it. The stallable origin it now needs is
// that file's machinery.

test("a warm boot never asks the network for the pack or the engine", async ({ page }) => {
  // The offline test (network.spec.ts) cannot tell depot-served from service-
  // worker-served: with both in play it passes either way, so it would go green
  // against a boot that secretly still depends on the SW being in its path. That
  // dependency is what the depot exists to remove — on a first visit the SW is
  // not controlling the page while the shell loads, and it claims the engine
  // worker mid-boot, so whether the pack reached the cache was a race.
  //
  // The sharp observable is the REQUEST, not the response. A bare fetch that the
  // SW happens to answer from its cache still issues a request; bytes the depot
  // already holds are read from storage and no request is made at all. So
  // counting requests on a warm boot separates the two, with the SW left
  // registered and doing its real job (serving the document).
  //
  // page.on("request") is the mechanism deliberately: it reports requests made
  // inside the ENGINE WORKER, which is where all of this happens. CDP does not —
  // a dedicated worker is a separate target.
  await boot(page);
  // Let the background stages finish on the FIRST visit, so their bytes are in
  // the depot before the warm boot we actually measure. `stage2 load` appearing
  // in the trace is the signal that stage 2 landed, rather than a guessed sleep.
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
    // NOTHING is exempt any more. The manifest used to be — it is the one pack
    // file with no version in its URL, so boot had to ask the network for it, and
    // on a stalled radio that cost up to the service worker's 3.5 s timebox before
    // a device holding all of scripture would open. The PIN replaced it: a
    // manifest stored on the device, written only after every file it names was
    // verified present. The live manifest is still fetched once per session by the
    // reconciler, but off the boot path, which is why this test measures only up
    // to the point the reader has text.
    if (u.pathname.includes("/pack/") || u.pathname.endsWith(".wasm")) asked.push(u.pathname);
  };
  page.on("request", listener);
  let untilText: string[] = [];
  try {
    await page.reload();
    await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 60_000 });
    await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/);
    // Snapshot HERE: text is on screen, and everything up to this point had to
    // come from storage. What the reconciler fetches afterwards is deliberate and
    // off the critical path.
    untilText = [...asked];
    // The background stages must be depot hits too, or the reader pays for
    // Strong's and the cross-references again on every single launch.
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
  // The engine parses each pack file into wasm memory, but the WASI shim's File
  // constructor COPIES what it is handed, so the in-memory home kept a second
  // copy of every byte forever — ~37 MB for the corpus cache alone, on a phone.
  // Files whose single reader has finished are dropped.
  //
  // The safety half of this test is the important half. Eviction is restricted to
  // data/ because `persistUserData` computes deletions by diffing the home
  // against IndexedDB: anything evicted from a USER directory would be deleted
  // from the reader's own storage on their next authoring write, permanently. And
  // data/kjv-notes.jsonl must survive because `load_study` re-reads it on every
  // one of those writes. So: author something, then check the margin notes and
  // the stock set are still there.
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

  // Now author, which makes the engine reload ALL study data from the home, and
  // confirm nothing the reader depends on was evicted out from under it.
  const after = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    // Authoring makes load_study rebuild ALL study data from the home, which is
    // the moment an over-eager eviction would show up.
    await s.engine.userNoteSet("Gen 1:1", "eviction probe", "2026-07-28T00:00:00Z");
    const [weaves, threads, mine, margin] = await Promise.all([
      s.engine.weaves(),
      s.engine.threads(),
      s.engine.userNote("Gen 1:1"),
      // The 1769 translators' margin notes come from data/kjv-notes.jsonl via
      // load_study — the file eviction must never touch. Gen 1:4 has one.
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

  // And the text still pages: the corpus decodes out of wasm memory, not the
  // node that was dropped.
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Rev", 22));
  await expect(page.locator(".subtitle")).toHaveText(/Revelation 22/, { timeout: 30_000 });
});

test("checking for an update cannot poison the cached shell", async ({ page }) => {
  // The live bug this pins: the update check fetched index.html as DATA, and the
  // service worker's network-first branch caches every ok response. So a session
  // that merely ASKED whether an update existed wrote a newer shell into the
  // cache while that build's /assets/* were absent — and the next offline launch
  // was served a document asking for a bundle nobody had. A white screen on a
  // device holding all of scripture.
  //
  // Two rules now hold: no-store responses are never cached, and index.html is
  // only cached for an actual navigation — recognised by PATHNAME, because for a
  // while it was recognised by full URL and `?as-data-probe` sailed past it.
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

    // DO NOT read the cache straight after the fetch. The service worker's
    // cache.put is fire-and-forget (not awaited, not in waitUntil), so an
    // immediate cache.match measures the race and not the rule: this test passed
    // on chromium against the LIVE bug because the read got there first, while on
    // WebKit the same read landed after the put and reported the truth.
    //
    // So the window is DERIVED from this machine, not a constant: a response the
    // worker IS supposed to cache goes first and is timed on its way in, then a
    // refused one gets an order of magnitude longer than that to show up anyway.
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
  // Navigations were cached under the URL REQUESTED, so every distinct deep link
  // (`/?at=Ps 23:1`, `/?church=…`) accumulated its own index.html that the sweep
  // never touched — un-versioned entries are exempt. Offline, one of those stale
  // copies would be served for that exact link, naming a bundle since pruned:
  // a white screen for shared links only, while the plain app worked fine.
  // The deep-link navigation has to be one the SERVICE WORKER actually sees, or
  // this proves nothing: on a first visit the SW is not controlling the page yet,
  // so the navigation never reaches its fetch handler and no entry is written
  // whatever the key logic says. Boot once to get the worker installed and in
  // control, THEN follow the shared link.
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
  // The precache used to be driven by this page's resource timeline, so it stored
  // whatever happened to load. A chunk imported lazily — for a screen the reader
  // had not opened — never appeared, and was simply missing offline. The build
  // now emits the shell's exact file list, and this asserts the depot holds ALL
  // of it, which the scrape could never guarantee.
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
  // Layout is measured in the WORKER over an OffscreenCanvas; the shell paints
  // the resulting display list here. So the worker needs the real EB Garamond in
  // its own FontFaceSet — with a fallback face it measures different advance
  // widths than the main thread paints, and lines wrap where they are not drawn.
  //
  // The failure is silent by design (a dead worker is worse than serif metrics),
  // which is exactly why it needs a test. This also pins down that the worker's
  // FontFace path accepts woff2 at all, which is what the faces became when they
  // were subsetted from 1.6 MB of TTF to 219 KB.
  await boot(page);
  const faces = await page.evaluate(async () => {
    const trace: [string, number][] = await (window as any).__plumbline.rpc.bootTrace();
    return trace.find(([l]) => l === "worker font faces")?.[1];
  });
  expect(faces, "the worker must load BOTH reader faces (roman + italic)").toBe(2);
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

test("background loading never starves the reader (worker-thread scheduling)", { tag: "@perf" }, async ({ page }) => {
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

// The companion to the test above, and the reason it needed one: that test's
// budget is `Math.max(400, chunk * 2.5)` where `chunk` is
// `Math.max(...warm steps)`. The budget is derived from the WORST chunk — so a
// phase that isn't sliced at all raises its own ceiling and the test can never
// fail on it. On the maintainer's phone (2026-07-28) the worst chunk was
// 54,859 ms, which set that budget to 137,147 ms.
//
// So: derive from the MEDIAN chunk instead. A slice is meant to be a budgeted
// slice; one chunk many times the typical one is not a slice, it is a block, and
// while it runs this thread answers no layout, no tap and no word study.
//
// HONEST ABOUT ITS REACH. The offender was warm phase 7, the "verses like this"
// SIF model — 54,859 ms on that phone against a ~300 ms median, and only ~226 ms
// against a ~6 ms median on a desktop, because its cost was allocation churn
// rather than arithmetic and a desktop absorbs that. The floor below keeps this
// test from flaking on a GC spike, and a desktop's 226 ms sat under it, which is
// why that phase also had a deterministic slicing guard in Rust.
//
// That feature was removed 2026-07-30 and its Rust guard with it, so THIS test is
// now the only thing watching for an unsliced background phase — which is the
// case it was written for: it catches the next one on whatever hardware runs it,
// without needing a slow device to notice.
test("no single background chunk may monopolise the engine worker", { tag: "@perf" }, async ({ page }) => {
  await boot(page);
  await settleBackground(page);

  const { worst, worstLabel, median, count } = await page.evaluate(async () => {
    const trace: [string, number][] = await (window as any).__plumbline.rpc.bootTrace();
    // Only the stages that CLAIM to be sliced. The stage-2 Strong's parse is one
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
  // Offline lives behind the Advanced disclosure now.
  await page.locator('[data-surface="settings"] details.advanced > summary').click();
  const download = page.getByRole("button", { name: "Download everything" });
  if (await download.isVisible().catch(() => false)) await download.click();
  await expect(page.getByText("Everything is on this device")).toBeVisible({ timeout: 120_000 });

  // Not just a label: every file the app actually READS must be on the device.
  //
  // `data/kjv.jsonl` is excluded, and its absence is asserted below rather than
  // ignored. The pack ships it, but with a parsed-corpus cache present no stage
  // ever fetches it — so counting it made the device permanently "incomplete"
  // and made this very button spend 2.4 MB on a file nothing opens.
  // Checked against the URLs the app itself uses — read from the manifest and
  // keyed the same way the loader keys them (per-file content hash), rather than
  // hand-rolled here. A test that rebuilds the URL scheme independently just
  // asserts that two copies of the scheme agree.
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

test("the welcome's verses are the corpus text, verbatim and instant", async ({ page }) => {
  // The welcome quotes scripture from literals rather than asking the engine
  // for ten verses one at a time — they used to pop in a beat after the page
  // (feedback 2026-07-27). A copy can drift, so this compares every quote on
  // screen against the corpus itself.
  await page.goto("/");
  await page.getByRole("button", { name: "New believer" }).click({ timeout: 90_000 });
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
  // One scan should hand over both (2026-07-27). The QR lives on the Share
  // screen since the menu rationalization — generated at render time, so
  // setting a church must change what it encodes: a longer payload needs a
  // bigger symbol.
  await boot(page);
  // Desktop puts the roles bar in the header, phones at the bottom — the
  // navigation landmark covers both.
  await page.getByRole("navigation").getByRole("button", { name: "Share", exact: true }).click();
  const card = page.locator('[data-surface="share app"]');
  await expect(card).toBeVisible();
  const modulesFor = async () =>
    card.locator("svg").getAttribute("viewBox").then((v) => Number(v!.split(" ")[2]));
  const plain = await modulesFor();
  // The card shows the HOST, never the full link: with a church attached the
  // URL runs off a phone screen (feedback 2026-07-27).
  await expect(card).toContainText("plumblinebible.org");

  await page.evaluate(() =>
    (window as any).__plumbline.setChurch({
      name: "Grace Bible Church",
      info: "Sundays 10am, 12 Long Street",
      url: "https://example.org",
    }),
  );
  // The named church is on the card, and the symbol grew to carry it.
  await expect(card).toContainText("Grace Bible Church");
  expect(await modulesFor()).toBeGreaterThan(plain);
});

// Sharing a PASSAGE is a QR carrying the passage, not the phone's share sheet
// carrying a wall of text (feedback 2026-07-27). Present is held up to someone
// in front of you, so what they scan must land them in the reader at the verse.
test("Present shares the passage as a QR whose link opens at the first verse", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Sharing the gospel" }).click({ timeout: 90_000 });
  await page.getByRole("button", { name: "Open the presentation screen" }).click();
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
  // The pin names EVERY file in the manifest and is written the moment the
  // engine opens — long before stage 2 and the analysis pack have finished
  // downloading. The last assertion here ("every file the pin names survived the
  // sweep") is therefore meaningless until they are actually on the device: a
  // file that never arrived is missing from the depot for reasons that have
  // nothing to do with prune, and the failure reads as "prune deleted a pinned
  // pack file", which is a lie.
  //
  // This test has always depended on the downloads happening to win that race,
  // and it lost the moment anything slowed them slightly (2026-07-28). Waiting is
  // the fix; the race was never part of what it means to test.
  await settleBackground(page);
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
  // ...and nothing else was collateral. The shell and every file the PIN names
  // must survive, or the next launch is broken or offline-dead.
  //
  // The keep-set is now the pin plus the shell manifest, not "entries whose ?v=
  // matches the current pack". That is what lets per-file hashes work at all, and
  // it also reclaims a file dropped from the pack entirely — which the old rule
  // could never do, because nothing referenced its version any more.
  expect(after).toContain("/index.html");
  const pinned = await page.evaluate(async () => {
    const hit = await caches.match(new URL("__depot/pack-pin.json", location.href).href, {
      ignoreVary: true,
    });
    const pin = hit ? await hit.json() : null;
    // Only the files the pin NAMES. An `optional` file the reader never asked
    // for is listed without a url — that is the pin saying "the pack offers
    // this, this device does not have it" — and prune is right not to keep it.
    return (pin?.files ?? [])
      .filter((f: { url?: string }) => f.url)
      .map((f: { url: string }) => "/" + f.url);
  });
  // A floor, not a count: what is asserted is that a pin exists and names the
  // whole pack, and the loop below is the real check. Kept well under the true
  // file count so that dropping an artifact (three of them went on 2026-07-30
  // with the concept map) fails here for no reason at all.
  expect(pinned.length, "there should be a pin naming the pack after a boot").toBeGreaterThan(30);
  for (const u of pinned) expect(after, `prune deleted a pinned pack file: ${u}`).toContain(u);
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
  //
  // The signal is the shell manifest's `buildId`, not a regex over index.html.
  // Scraping the document meant the SW cached a newer shell whose bundles did not
  // exist yet — a white screen on a device holding all of scripture.
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
  await expect(page.getByRole("button", { name: "New believer" })).toBeVisible();
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
  await page.getByRole("button", { name: "Open the Bible" }).click();
  await expect(page.locator(".subtitle")).toContainText("John 1");

  // Back to it from the ≡ utilities, without changing anything.
  await page.getByLabel("Menu").click();
  await page.getByRole("button", { name: "Welcome" }).click();
  await expect(page.getByText("I'm glad you're curious about the Bible")).toBeVisible();
  await page.getByRole("button", { name: "Close" }).click();
  await expect(page.locator(".pane canvas").first()).toBeVisible();

  // …and it survives a relaunch, since it's the reader's own welcome now.
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await page.getByLabel("Menu").click();
  await expect(page.getByRole("button", { name: "Welcome" })).toBeVisible();
});

test("a Present link offers only the two paths it was meant for", async ({ page }) => {
  // Handed to someone in person: new believer or curious. Setting up study
  // tiers is not what that moment is for.
  await page.goto("/?start=new");
  await expect(page.getByRole("button", { name: "New believer" })).toBeVisible({ timeout: 90_000 });
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
  // "One row" means the visible children share a row AS EACH OTHER — compared
  // among themselves, not against the header's own top. The original form used the
  // container's top with a 24px tolerance, which quietly also asserted a bar height
  // and so went red the moment the bar was made bigger on purpose (2026-07-29).
  // Children with display:none are skipped: their rects are all zeros and would
  // drag the spread to the full bar height whatever the layout did.
  const oneRow = () =>
    page.locator("header").evaluate((h) => {
      const tops = [...h.children]
        .filter((c) => c.getBoundingClientRect().height > 0)
        .map((c) => c.getBoundingClientRect().top);
      return tops.length > 0 && Math.max(...tops) - Math.min(...tops) < 24;
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
  await page.getByRole("button", { name: "New believer" }).click({ timeout: 90_000 });
  const findChurch = page.locator(".welcome p", { hasText: "Find a church" });
  await expect(findChurch).toContainText("shared with you by");
  await expect(findChurch).toContainText("Grace Bible Church");
  await expect(findChurch).toContainText("Sundays 10AM");
  await expect(findChurch.getByRole("link", { name: /Visit Grace Bible Church/ })).toHaveAttribute(
    "href",
    "https://example.org",
  );
});

test("with no church shared, the welcome keeps its general advice", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New believer" }).click({ timeout: 90_000 });
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
  await page.getByRole("button", { name: "Study", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Memorize/ }).click();
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
  await page.getByRole("button", { name: "Study", exact: true }).click();
  await page.locator(".ex-card", { hasText: /^Memorize/ }).click();
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
