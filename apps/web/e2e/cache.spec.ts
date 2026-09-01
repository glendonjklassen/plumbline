import { expect, test, type Page } from "@playwright/test";

// The read-through cache in state/session.svelte.ts — three ways it misbehaved, each asserted
// where nothing else can interfere.
//
// Unit assertions over the cache rather than repaint checks: the background warm calls the
// whole-cache `invalidate()` a few seconds into a fresh profile and refreshes everything as a side
// effect, which let an earlier end-to-end version pass with the fix reverted. Every raceable
// assertion below runs inside one page.evaluate, so the page's single thread makes interleaving
// impossible rather than unlikely.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/**
 * Wait out the background pipeline: `onCoreReady`, `onWarmReady` and `onRndReady` all call the
 * whole-cache `invalidate()`, which would empty the cache underneath a test measuring how full it
 * is. A boot trace that has stopped growing is the signal — every warm and analysis step appends
 * one entry, and the opt-in analysis tiers mean `rndState` never reaches "ready" on a fresh
 * profile at all.
 *
 * Polled from Node, not `page.waitForFunction`: reading the boot trace is an async RPC, and an
 * async predicate returns a promise, which is truthy, so the poller fulfils on its first
 * invocation and the helper waits for nothing. Anything awaiting an RPC polls from here.
 */
async function settleBackground(page: Page): Promise<void> {
  const traceLen = () =>
    page.evaluate(async () => ((await (window as any).__plumbline.rpc.bootTrace()) ?? []).length);
  const deadline = Date.now() + 120_000;
  let prev = -1;
  for (;;) {
    const n = await traceLen();
    if (n === prev && n > 10) return;
    if (Date.now() > deadline) {
      throw new Error(`the background pipeline never stopped appending boot-trace entries (${n})`);
    }
    prev = n;
    await new Promise((r) => setTimeout(r, 1500));
  }
}

// Fails against the bug: with `isPinned` testing `key.startsWith(m + " ")` rather than
// `key.startsWith(m + KEY_SEP)`, "the TOC must survive an invalidate" goes red.
test("invalidate keeps the corpus immutables and drops the derived reads", async ({ page }) => {
  await boot(page);

  // `invalidate()` keeps the corpus-derived reads, because wiping them made navigation clamp
  // against an empty TOC mid-refill. It tested the key prefix `"toc "` — with a space — while
  // `q()` builds keys as `${method}\0${JSON.stringify(args)}`, so the exemption never matched and
  // every ready/authored event dropped the TOC and the canon segments with the study reads.
  const primed = () =>
    page.evaluate(() => {
      const s = (window as any).__plumbline;
      return {
        toc: s.q("toc") !== null,
        canon: s.q("canonSegments") !== null,
        threads: s.q("threads") !== null,
      };
    });
  await expect.poll(async () => (await primed()).toc, { timeout: 30_000 }).toBe(true);
  await expect.poll(async () => (await primed()).canon, { timeout: 30_000 }).toBe(true);
  await expect.poll(async () => (await primed()).threads, { timeout: 30_000 }).toBe(true);

  // Invalidate and read back in one evaluate, before any refetch or background event can land.
  const after = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.invalidate();
    return {
      toc: s.q("toc") !== null,
      canon: s.q("canonSegments") !== null,
      threads: s.q("threads") !== null,
    };
  });

  expect(after.toc, "the TOC must survive an invalidate — navigation clamps against it").toBe(true);
  expect(after.canon, "the canon segments must survive — the strip and the maps read them").toBe(true);
  expect(after.threads, "a study read is dropped and will refetch").toBe(false);
});

// Fails against the bug: with `#store`'s eviction unreachable the peak is 1,193 — one entry per
// chapter of the canon — against a cap of 512.
test("the cache stays bounded under sustained use", async ({ page }) => {
  await boot(page);
  // Not for the measurement (`peak` below is invalidate-proof by construction) but because a boot
  // still in flight makes the flood race the pack loader for the worker.
  await settleBackground(page);

  // The cache grew for the life of the tab. Several call sites mint a key per interaction
  // (`searchBlocks` per keystroke, `wordStudyBlocks` per tapped word, `verse` per verse of a
  // preview), so this drives the cheapest of them: one verse read per chapter of the canon.
  //
  // Measured as a peak and as the longest uninterrupted run of inserts, because the final size
  // alone is not evidence — a background `invalidate()` landing mid-flood drops hundreds of
  // entries and would leave an unbounded cache looking bounded. It cannot lower the peak, and
  // `run` resets whenever the size falls, so `maxRun > cap` proves more distinct keys than the
  // bound really were asked for with nothing wiping them in between.
  const flood = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const refs: string[] = [];
    for (const b of s.q("toc").books) for (let c = 1; c <= Number(b.chapters); c++) refs.push(`${b.id} ${c}:1`);
    let peak = 0;
    let run = 0;
    let maxRun = 0;
    let prev = s.cacheSize;
    for (const r of refs) {
      await s.fetchQ("verse", r);
      const size = s.cacheSize;
      // A drop of more than one is an invalidation, not an eviction.
      run = size < prev - 1 ? 0 : run + 1;
      if (run > maxRun) maxRun = run;
      if (size > peak) peak = size;
      prev = size;
    }
    // Read back in the same turn: the loop's last continuation is a microtask, so no worker
    // message can be delivered before this return.
    return {
      asked: refs.length,
      peak,
      maxRun,
      cap: s.constructor.CACHE_CAP as number,
      newestKept: s.q("verse", refs[refs.length - 1]) !== null,
      oldestEvicted: s.q("verse", refs[0]) === null,
      tocKept: s.q("toc") !== null,
    };
  });

  expect(flood.asked, "one verse per chapter should be the whole canon").toBeGreaterThan(1000);
  expect(flood.maxRun, "the flood must ask for more distinct keys than the bound, uninterrupted").toBeGreaterThan(
    flood.cap,
  );
  expect(flood.peak, "the cache must stay inside its own bound").toBeLessThanOrEqual(flood.cap);
  // LRU, not FIFO from the wrong end: what was read last is what is still there.
  expect(flood.newestKept, "the most recent read must survive eviction").toBe(true);
  expect(flood.oldestEvicted, "the least recently read must be the one evicted").toBe(true);
  expect(flood.tocKept, "eviction must not take the pinned TOC either").toBe(true);
});

// Fails against the bug: drop `#memoMarks`'s `if (same) return prev;` so every call returns a
// fresh Set, and "an epoch bump that changed nothing must not hand back a different Set" reddens.
test("a no-op epoch bump does not re-mint the gutter marks", async ({ page }) => {
  await boot(page);

  // The pane's paint effect tracks the weave dots and note marks, so a new Set holding the same
  // verse numbers costs a full repaint. Every invalidation bumps `studyEpoch`, so the derived
  // re-runs on all of them and only content memoization makes "nothing changed" cost nothing.
  await expect
    .poll(async () => page.evaluate(() => ((window as any).__plumbline.q("linkPairs")?.pairs ?? []).length), {
      timeout: 60_000,
      message: "the stock weaves should give this profile some link pairs",
    })
    .toBeGreaterThan(0);

  // A chapter that really has dots: an all-empty set would catch a re-minted Set but would not
  // exercise the content comparison itself.
  const target = await page.evaluate(() => {
    const p = (window as any).__plumbline.q("linkPairs").pairs[0];
    return { book: p.aBook as string, chapter: p.aChapter as number };
  });

  const dots = await page.evaluate((t) => {
    const s = (window as any).__plumbline;
    const first = s.weaveDots(t.book, t.chapter);
    (window as any).__dots = first;
    // A bump with the source still cached: exactly what `studyEpoch++` alone does.
    s.studyEpoch++;
    const afterBump = s.weaveDots(t.book, t.chapter);
    // The real event: invalidate drops `linkPairs`, so the rebuild has no source for a beat.
    s.invalidate();
    s.studyEpoch++;
    const whileRefetching = s.weaveDots(t.book, t.chapter);
    return {
      size: first.size,
      afterBump: afterBump === first,
      whileRefetching: whileRefetching === first,
    };
  }, target);

  expect(dots.size, "the sample chapter should have dots to hold on to").toBeGreaterThan(0);
  expect(dots.afterBump, "an epoch bump that changed nothing must not hand back a different Set").toBe(true);
  expect(dots.whileRefetching, "the marks must be held through the refetch, not blinked off").toBe(true);

  // The refetch lands with identical content: still the same Set, so still no repaint.
  await expect
    .poll(
      async () =>
        page.evaluate((t) => {
          const s = (window as any).__plumbline;
          return s.q("linkPairs") !== null && s.weaveDots(t.book, t.chapter) === (window as any).__dots;
        }, target),
      { timeout: 30_000, message: "a refetch with the same content must not re-mint the dots" },
    )
    .toBe(true);

  // The note marks, same mechanism, with a note written here so the set is genuinely non-empty.
  const ref = `${target.book} ${target.chapter}:1`;
  expect(
    await page.evaluate((r) => (window as any).__plumbline.author("userNoteSet", r, "test", "2026-07-29T12:00:00Z"), ref),
    "the note should save",
  ).toBeNull();
  await expect
    .poll(
      async () =>
        page.evaluate((t) => (window as any).__plumbline.noteVerses(t.book, t.chapter).size, target),
      { timeout: 30_000, message: "the authored note should reach the mark set" },
    )
    .toBeGreaterThan(0);

  const notes = await page.evaluate((t) => {
    const s = (window as any).__plumbline;
    const first = s.noteVerses(t.book, t.chapter);
    s.studyEpoch++;
    const afterBump = s.noteVerses(t.book, t.chapter);
    s.invalidate();
    s.studyEpoch++;
    return { afterBump: afterBump === first, whileRefetching: s.noteVerses(t.book, t.chapter) === first };
  }, target);
  expect(notes.afterBump, "the note marks must survive a no-op epoch bump too").toBe(true);
  expect(notes.whileRefetching, "and be held through their refetch").toBe(true);
});
