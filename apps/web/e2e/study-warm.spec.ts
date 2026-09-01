import { expect, test, type Page } from "@playwright/test";

// The Study hub's progress band is built from four engine reads (plans, cards due, suggested
// weaves, the reading map), warmed in the background — at boot idle, and again after anything
// that empties the cache they live in.
//
// These tests watch the cache, not a stopwatch: "the answers are already there when the hub
// mounts" means the same thing on a fast machine and a slow one; a millisecond budget would
// not.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/**
 * Wait out the background pipeline, as cache.spec.ts and search.spec.ts do.
 *
 * Each boot stage (core, warm, R&D) empties the read-through cache and schedules a warm of
 * its own, so a test that writes while they are still landing cannot tell its re-warm from
 * theirs. Polled from Node: an async predicate handed to `page.waitForFunction` returns a
 * promise — truthy — so the poller fulfils immediately and waits for nothing.
 */
async function settleBackground(page: Page): Promise<void> {
  const traceLen = () =>
    page.evaluate(async () => ((await (window as any).__plumbline.rpc.bootTrace()) ?? []).length);
  const deadline = Date.now() + 120_000;
  let prev = -1;
  for (;;) {
    const n = await traceLen();
    if (n === prev && n > 10) return;
    if (Date.now() > deadline) throw new Error(`the background pipeline never settled (${n})`);
    prev = n;
    await new Promise((r) => setTimeout(r, 1500));
  }
}

/** The reads the hub's band is built from, keyed exactly as it asks for them. */
async function bandCached(page: Page): Promise<Record<string, boolean>> {
  return await page.evaluate(() => {
    const s = (window as any).__plumbline;
    // `dayStamp` is one definition shared by the hub, the navigator and the warm —
    // computing it differently here would test a key nothing uses.
    const day = new Date().toISOString().replace(/\.\d{3}Z$/, "Z").slice(0, 10) + "T12:00:00Z";
    return {
      plans: s.q("plans", "") !== null,
      memoryDue: s.q("memoryDue", day) !== null,
      suggestedWeaves: s.q("suggestedWeaves") !== null,
      readingBooks: s.q("readingBooks", day) !== null,
    };
  });
}

// Mutation: drop `this.scheduleStudyWarm()` from `rpc.onWarmReady` — red, every band read is
// still uncached when the hub opens. The warm hangs off the pipeline finishing rather than
// boot idle, because eight engine reads in front of the corpus opening on the one worker
// thread cost cold starts minutes (Session.STUDY_WARM_MIN_GAP_MS), so this test waits.
test("the Study hub's progress is already loaded before you open it", async ({ page }) => {
  await boot(page);
  // Settle the pipeline first, and step off the Read screen. Each boot stage empties the
  // cache and re-warms at idle, and the product's floor is that a reader arriving inside
  // that gap gets the placeholder — so sampling "all cached" mid-stage asserts more than
  // the product promises. Reading also accrues dwell, and a dwell write invalidates
  // `plans`, which keeps a second race open.
  await settleBackground(page);
  await page.evaluate(() => ((window as any).__plumbline.screen = "share"));
  await page.waitForTimeout(1500);

  // The warm runs at idle, and nothing on screen waits for it, so the test has to.
  await expect
    .poll(async () => Object.values(await bandCached(page)).every(Boolean), { timeout: 60_000 })
    .toBe(true);

  // Arriving at the hub asks the engine for nothing: every question it has was answered
  // while the reader was still in the text.
  const asked: string[] = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    const seen: string[] = [];
    (window as any).__asked = seen;
    const call = s.rpc.call.bind(s.rpc);
    s.rpc.call = (method: string, ...args: unknown[]) => {
      seen.push(method);
      return call(method, ...args);
    };
    return seen;
  });
  await page.getByRole("button", { name: "Study" }).first().click();
  await expect(page.locator("section.band")).toBeVisible({ timeout: 30_000 });

  const band = ["plans", "memoryDue", "suggestedWeaves", "readingBooks"];
  const refetched = (await page.evaluate(() => (window as any).__asked as string[])).filter((m) =>
    band.includes(m),
  );
  expect(refetched, `the hub re-asked for ${JSON.stringify(refetched)}`).toEqual([]);
  void asked;

  // And the band draws its real rows, not the same-shaped placeholder it holds
  // while its reads are in flight.
  await expect(page.locator("section.band .skeleton")).toHaveCount(0);
  await expect(page.locator("section.band .settled")).toBeVisible();
});

// Mutation: drop `this.scheduleStudyWarm()` from `rpc.onAuthored` — red, the band's reads are
// never re-asked after a write. This watches for the re-warm happening rather than for the
// cache being briefly empty: that gap is what the fix closes, so asserting on it would be
// asserting on a race, and probing through `q()` re-fills the cache it is inspecting.
test("a write re-warms the hub instead of leaving it cold", async ({ page }) => {
  await boot(page);
  // Every boot stage re-warms too; settle them first so what this observes can only be the
  // write's own re-warm.
  await settleBackground(page);
  await expect
    .poll(async () => Object.values(await bandCached(page)).every(Boolean), { timeout: 60_000 })
    .toBe(true);

  // Off the Read screen first: reading accrues dwell, a dwell report is itself a write and
  // re-warms on the same reasoning, so a test that stays in the text cannot tell the
  // authoring warm from the reading one. Share reads none of the three below.
  await page.evaluate(() => ((window as any).__plumbline.screen = "share"));
  await page.waitForTimeout(1500);

  // Record what the shell asks the engine from here on — nowhere near Study.
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    const seen: string[] = [];
    (window as any).__afterWrite = seen;
    const call = s.rpc.call.bind(s.rpc);
    s.rpc.call = (method: string, ...args: unknown[]) => {
      seen.push(method);
      return call(method, ...args);
    };
  });

  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    // kind "verse", value a refKey — the shape every caller in the shell uses.
    await s.author("tagAdd", "warm-test", "verse", "John 3:16", null, new Date().toISOString());
  });

  // The write emptied the cache the hub reads from; the re-warm fills it again
  // on its own.
  await expect
    .poll(
      async () => {
        const seen: string[] = await page.evaluate(() => (window as any).__afterWrite);
        // Not `plans` (the Read screen's chip re-fetches it on any write) and not
        // `memoryDue` (the app badge asks for it directly in the same handler). These
        // three are the hub's alone, so seeing them means the warm ran.
        return ["suggestedWeaves", "readingBooks", "tags"].every((m) => seen.includes(m));
      },
      { timeout: 60_000 },
    )
    .toBe(true);

  // And what it warmed is the post-write answer: the new tag is in the count the hub will
  // show, without the reader having opened it. Polled, because the warm walks its reads in
  // order and the card counts come after the band.
  await expect
    .poll(
      async () =>
        await page.evaluate(() =>
          ((window as any).__plumbline.q("tags")?.tags ?? []).map((t: any) => String(t.name)),
        ),
      { timeout: 30_000 },
    )
    .toContain("warm-test");
});
