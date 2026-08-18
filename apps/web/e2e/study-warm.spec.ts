import { expect, test, type Page } from "@playwright/test";

// The Study hub's progress band used to be fetched when the reader ARRIVED:
// four engine reads (plans, cards due, suggested weaves, the reading map) that
// all started on first render, so the band held a placeholder until they
// landed and the numbers appeared a beat later. Those reads are warmed in the
// background now — at boot idle, and again after anything that empties the
// cache they live in.
//
// What these tests watch is the CACHE, not a stopwatch: "the answers are
// already there when the hub mounts" means the same thing on a fast machine and
// a slow one, which a millisecond budget would not.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/**
 * Wait out the background pipeline, as cache.spec.ts and search.spec.ts do.
 *
 * The boot stages (core, warm, R&D) each empty the read-through cache and each
 * schedule a warm of their own. A test that writes while they are still landing
 * cannot tell ITS re-warm from theirs — which is how the second test below
 * first passed with the behaviour it describes removed.
 *
 * POLLED FROM NODE: reading the trace is an async RPC, and an async predicate
 * handed to `page.waitForFunction` returns a promise — truthy — so the poller
 * fulfils immediately and waits for nothing.
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
    // `dayStamp` is one definition shared by the hub, the navigator and the
    // warm — computing it differently here would test a key nothing uses.
    const day = new Date().toISOString().replace(/\.\d{3}Z$/, "Z").slice(0, 10) + "T12:00:00Z";
    return {
      plans: s.q("plans", "") !== null,
      memoryDue: s.q("memoryDue", day) !== null,
      suggestedWeaves: s.q("suggestedWeaves") !== null,
      readingBooks: s.q("readingBooks", day) !== null,
    };
  });
}

// MUTATION: drop `this.scheduleStudyWarm()` from `rpc.onWarmReady`. Red: every
// band read is still uncached when the hub opens, so the placeholder is what
// the reader gets.
//
// The warm hangs off the pipeline finishing, NOT off boot idle — a warm at boot
// put eight engine reads in front of the corpus opening on the one worker
// thread and cost cold starts minutes (see Session.STUDY_WARM_MIN_GAP_MS). That
// is why this test waits rather than expecting the cache to be warm instantly.
test("the Study hub's progress is already loaded before you open it", async ({ page }) => {
  await boot(page);
  // SETTLE THE PIPELINE FIRST, and step off the Read screen — the two lessons
  // the test below this one already carries, which this one never got because
  // it happened to pass. Each boot stage (core → warm → R&D) EMPTIES the cache
  // and re-warms at idle; the product's documented floor is that a reader who
  // arrives inside that gap gets the placeholder. Sampling "all cached" while
  // the stages are still landing asserts more than the product promises — on a
  // loaded CI runner the R&D stage finished after the warm, the click fell in
  // its gap, and the hub honestly re-asked for plans and suggestedWeaves. And
  // reading accrues dwell, a dwell write invalidates `plans`, so staying on the
  // text keeps a second race open after the first is closed.
  await settleBackground(page);
  await page.evaluate(() => ((window as any).__plumbline.screen = "share"));
  await page.waitForTimeout(1500);

  // The warm runs at idle. Nothing on screen waits for it, which is exactly
  // why the test has to.
  await expect
    .poll(async () => Object.values(await bandCached(page)).every(Boolean), { timeout: 60_000 })
    .toBe(true);

  // Arriving at the hub asks the engine for NOTHING: every question it has was
  // answered while the reader was still in the text.
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

// MUTATION: drop `this.scheduleStudyWarm()` from `rpc.onAuthored`. Red: the
// band's reads are never re-asked after a write, so the next visit to Study
// waits for exactly the numbers that write changed.
//
// This watches for the re-warm HAPPENING rather than for the cache being
// briefly empty. The gap between a write and the re-warm is what the fix
// exists to close, so asserting on it would be asserting on a race — and the
// obvious probe makes it worse, because reading through `q()` re-fills the
// cache it is inspecting.
test("a write re-warms the hub instead of leaving it cold", async ({ page }) => {
  await boot(page);
  // Every boot stage re-warms too; settle them first so what this observes can
  // only be the write's own re-warm.
  await settleBackground(page);
  await expect
    .poll(async () => Object.values(await bandCached(page)).every(Boolean), { timeout: 60_000 })
    .toBe(true);

  // OFF THE READ SCREEN FIRST. Reading accrues dwell, a dwell report is itself
  // a write, and it re-warms on the same reasoning — so a test that stays in
  // the text cannot tell the authoring warm from the reading one, and passed
  // with the authoring warm deleted. Share reads none of the three below.
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
        // NOT `plans` (the Read screen's chip re-fetches it on any write) and
        // NOT `memoryDue` (the app badge asks for it directly in the same
        // handler). These three are the hub's alone, so seeing them means the
        // warm ran rather than something else refreshing itself.
        return ["suggestedWeaves", "readingBooks", "tags"].every((m) => seen.includes(m));
      },
      { timeout: 60_000 },
    )
    .toBe(true);

  // And what it warmed is the POST-write answer — the new tag is in the count
  // the hub will show, without the reader having opened it.
  //
  // POLLED, because the warm walks its reads in order and the card counts come
  // after the band: reading this the instant the band landed caught `tags`
  // mid-warm and failed about one run in three.
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
