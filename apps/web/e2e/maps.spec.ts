import { expect, test, type Page } from "@playwright/test";

// The analytical maps — Weave map (ChordMap) and Constellation — had no test of
// any kind. ~500 lines of canvas drawing, hit-testing and paging, reached from
// Explore, entirely unexercised (v1.0.0 audit, item I).
//
// There was a third, the concept map, and this file covered it the same three
// ways. Both it and its test were removed 2026-07-30: it drew the concept
// embedding, which is no longer shipped. The two here are weave visualisations
// and are unaffected.
//
// WHAT A SMOKE TEST HAS TO DO HERE. These surfaces DRAW. "The popup opened" is
// not a passing grade, because every way they break leaves the popup open: they
// draw nothing, they draw outside the box, or they throw the moment a model
// arrives late. So every map here is asserted three ways —
//
//   1. its canvas really has ink on it, in the places that map's own drawing
//      contract puts ink (the canon axis spans the full width; ribbons arc above
//      it; lane names live in the left gutter);
//   2. something the reader points at answers — a hovered node names its verse,
//      a tapped book opens that book. A hit test can only answer if `paint()`
//      really ran, because paint() is what fills the position tables the hit
//      test reads;
//   3. the page logged nothing broken, and the surface can be opened,
//      interacted with and dismissed without taking the reader with it.
//
// The pixel reads are deliberately about STRUCTURE (which regions carry ink, and
// whether every column does) rather than about a total that happens to be right
// today. Measured on this machine 2026-07-29 for the record: weave map 8.9% ink,
// constellation 8.5%, both 64/64 columns.
//
// WHAT IT FOUND, on the first run. Both Explore maps are broken today:
//
//   * Weave map draws NOTHING the first time it is opened. ChordMap's $effect
//     guards `!canvas || !host` but not `!model`, and `model` is null on the
//     first run (the read-through cache answers null and fires the fetch). So
//     paint() dereferences `m.bookCount`, the effect throws, Svelte disables it,
//     and when the model lands a beat later nothing repaints. Constellation has
//     the `|| !model` guard; ChordMap does not.
//   * Constellation NEVER renders, at all. `pins` is `$state<number[]>([])` — a
//     Svelte proxy — and `q("constellation", page, pins)` hands it to
//     `postMessage`, which cannot structured-clone a Proxy: "DataCloneError:
//     [object Array] could not be cloned". Every fetch fails, so the model is
//     forever null and the frame sits on "— building —" for good.
//
// Both are one-liners in files this spec does not own, so the two tests below are
// RED until they land. `maps/ChordMap.svelte` needs `|| !model` in its effect
// guard; `maps/Constellation.svelte` needs `$state.snapshot(pins)` (or
// `[...pins]`) at the q() call. Verified by neutralising each cause from inside
// this file and watching the same assertions go green — see MUTATION notes below.
//
// TWO TRAPS THIS FILE AVOIDS, both learned here:
//
//   * `page.waitForFunction` with an ASYNC predicate returns on its first tick —
//     the promise itself is the truthy value. Measured: 19 ms, one call, before
//     the warm had even started. Anything that has to wait for the background
//     pipeline uses `expect.poll`, which awaits what it is given.
//   * A failed engine read is reported by `q()` as a console WARNING and by
//     nothing else. That warning was the constellation's only symptom. A watcher
//     that listens for errors alone calls a permanently blank map a pass.

/** The maps are wide (the constellation is 1200×640). A 1280×720 window clips
 *  them against MapFrame's own `max-height: 82vh`, which is correct behaviour
 *  but not what is under test here, so give the popup room to be itself. */
const VIEWPORT = { width: 1280, height: 900 };

/** How many columns the ink profile samples across the canvas. */
const COLUMNS = 64;

/** Everything the page said that it should not have. */
function watchPage(page: Page): string[] {
  const problems: string[] = [];
  page.on("pageerror", (e) => problems.push(`uncaught: ${String(e.message).split("\n")[0]}`));
  page.on("console", (m) => {
    const text = m.text().split("\n")[0];
    // Errors always; warnings only from the app itself. `q()` reports a failed
    // engine read with `console.warn("[plumbline] <method> failed:")` and does
    // nothing else about it, which is the whole visible trace of the
    // constellation's DataCloneError. Browser chatter — including the Canvas2D
    // "willReadFrequently" hint that this file's own pixel reads provoke — is not
    // the app's and is not a finding.
    if (m.type() === "error" || (m.type() === "warning" && text.includes("[plumbline]")))
      problems.push(`${m.type()}: ${text}`);
  });
  return problems;
}

/** Boot to the reader. The analysis tiers are left OFF — both maps here are
 *  built from weaves, which ship in the core pack. (This helper used to take a
 *  `tiers` flag for the concept map, whose route in was machine-tier gated; it
 *  went with that map on 2026-07-30.) */
async function boot(page: Page): Promise<void> {
  await page.setViewportSize(VIEWPORT);
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/**
 * Wait out the background pipeline before opening a map.
 *
 * Not politeness — correctness. Both `onWarmReady` and `onRndReady` call the
 * whole-cache `invalidate()`, which drops the map's model out from under an open
 * popup and puts "— building —" back over its canvas for a beat. A pixel read
 * that lands in that beat sees blank paper and blames the map.
 *
 * `expect.poll`, NOT `page.waitForFunction`: an async predicate handed to
 * waitForFunction is a promise, promises are truthy, and it returns on the first
 * tick having waited for nothing.
 */
async function settleBackground(page: Page): Promise<void> {
  let previous = -1;
  let sawWarm = false;
  await expect
    .poll(
      async () => {
        const trace: [string, number][] = await page.evaluate(() =>
          (window as any).__plumbline.rpc.bootTrace(),
        );
        // Every warm and analysis chunk appends one timed entry, so a trace that
        // has stopped growing says "nothing further is coming". The warm must
        // have STARTED too: early in boot the trace is briefly static between
        // stage 2 and the first warm step, and settling there would settle
        // nothing.
        sawWarm ||= trace.some(([label]) => label.startsWith("warm step"));
        const quiet = trace.length === previous && sawWarm;
        previous = trace.length;
        return quiet;
      },
      { timeout: 180_000, intervals: [1000], message: "the background pipeline should go quiet" },
    )
    .toBe(true);
}

/** The open map's canvas.
 *
 *  `.popup` and `.host` belong to MapFrame alone today, but they are classes and
 *  not `data-surface` attributes, so every use asserts there is exactly ONE
 *  popup on screen first. If a second component ever adopts `.popup`, this says
 *  so instead of quietly measuring the wrong canvas — which is exactly how the
 *  destination-bar sweep spent a run measuring the passage navigator while
 *  reporting on Settings. */
function mapCanvas(page: Page) {
  return page.locator(".popup .host canvas");
}

/** Open a map and wait until it has its model (the frame stops saying
 *  "— building —"). Returns the canvas. */
async function openedMap(page: Page, title: string | RegExp): Promise<void> {
  await expect(page.locator(".popup"), "exactly one popup should be on screen").toHaveCount(1);
  await expect(page.locator(".popup .bar .title")).toHaveText(title);
  await expect(mapCanvas(page)).toBeVisible();
  await expect(
    page.locator(".popup .wait"),
    `"${title}" never stopped saying "— building —": its model never arrived`,
  ).toHaveCount(0, { timeout: 30_000 });
}

interface InkProfile {
  /** For the failure message: how big the thing we measured was. */
  size: string;
  /** Fraction of sampled pixels that are neither the popup's paper nor unpainted. */
  ink: number;
  /** Fraction still transparent — a canvas nobody drew on at all. */
  unpainted: number;
  /** How many of COLUMNS sampled columns carry any ink. */
  columns: number;
  /** Ink fraction inside each named region, given as [x0, y0, x1, y1] fractions. */
  bands: Record<string, number>;
}

/**
 * Read the map's canvas back and describe where the ink is.
 *
 * Every map fills its whole canvas with the popup's paper (#f2eee6) before it
 * draws, which gives three distinguishable states per pixel: unpainted
 * (transparent — the canvas was never touched), paper, and ink. The distinction
 * matters: a canvas that was never drawn on at all reads as "not paper" on a
 * naive colour comparison, so a blank map would have scored 100% ink.
 */
async function inkProfile(page: Page, bands: Record<string, [number, number, number, number]> = {}): Promise<InkProfile> {
  return page.evaluate(
    ({ bands, COLUMNS }) => {
      const canvas = document.querySelector(".popup .host canvas") as HTMLCanvasElement;
      const { width: W, height: H } = canvas;
      const data = canvas.getContext("2d")!.getImageData(0, 0, W, H).data;
      // #f2eee6, the popup paper every map lays down first.
      const kind = (i: number): "unpainted" | "paper" | "ink" =>
        data[i + 3] < 255
          ? "unpainted"
          : Math.abs(data[i] - 242) + Math.abs(data[i + 1] - 238) + Math.abs(data[i + 2] - 230) > 8
            ? "ink"
            : "paper";
      let ink = 0;
      let unpainted = 0;
      let total = 0;
      const columns = new Set<number>();
      for (let y = 0; y < H; y += 2)
        for (let x = 0; x < W; x += 2) {
          total++;
          const k = kind((y * W + x) * 4);
          if (k === "ink") {
            ink++;
            columns.add(Math.floor((x / W) * COLUMNS));
          } else if (k === "unpainted") unpainted++;
        }
      const band = ([x0, y0, x1, y1]: [number, number, number, number]): number => {
        let n = 0;
        let seen = 0;
        for (let y = Math.floor(y0 * H); y < Math.floor(y1 * H); y += 2)
          for (let x = Math.floor(x0 * W); x < Math.floor(x1 * W); x += 2) {
            seen++;
            if (kind((y * W + x) * 4) === "ink") n++;
          }
        return seen ? n / seen : 0;
      };
      const out: any = {
        size: `${canvas.clientWidth}×${canvas.clientHeight} css, ${W}×${H} device`,
        ink: ink / total,
        unpainted: unpainted / total,
        columns: columns.size,
        bands: {},
      };
      for (const [name, rect] of Object.entries(bands)) out.bands[name] = band(rect as any);
      return out;
    },
    { bands, COLUMNS },
  );
}

/**
 * Wait until the map has actually put ink on its canvas, then profile it.
 *
 * The canvas paints in a `$effect` a frame after the model lands, so this
 * retries rather than snapshotting — and when a map never paints, this poll's
 * failure IS the finding, which is why it says so in plain words.
 */
async function drawnProfile(
  page: Page,
  bands: Record<string, [number, number, number, number]>,
  problems: string[] = [],
): Promise<InkProfile> {
  await expect
    .poll(async () => (await inkProfile(page)).ink, {
      timeout: 15_000,
      // Whatever the page said is folded into the message, because the cause of
      // a blank canvas is almost always sitting in that list already: the weave
      // map's is an uncaught "Cannot read properties of null (reading
      // 'bookCount')" thrown out of its own paint effect.
      message:
        "the map put no ink on its canvas at all — it drew nothing" +
        (problems.length ? `\n  the page logged: ${distinct(problems).join("; ")}` : ""),
    })
    .toBeGreaterThan(0);
  return inkProfile(page, bands);
}

/** Model coordinates → page coordinates, the way each map's own hit test
 *  inverts them: x through `rect.width / W`, y through `rect.height / H`, at
 *  1× zoom. Getting this wrong is how "the map drew off-screen" hides. */
async function pointAt(page: Page, x: number, y: number, W: number, H: number): Promise<[number, number]> {
  const box = (await mapCanvas(page).boundingBox())!;
  return [box.x + (x / W) * box.width, box.y + (y / H) * box.height];
}

/** Explore → one of its cards. The real route a reader takes to the two
 *  library-wide maps. */
async function openFromExplore(page: Page, card: RegExp): Promise<void> {
  await page.locator("nav.browse").getByRole("button", { name: "Explore" }).click();
  await page.getByRole("button", { name: card }).click();
}

const distinct = (xs: string[]): string[] => [...new Set(xs)];

// ─────────────────────────────────────────────────────────────────────────────

test("the weave map draws the canon and its ribbons", async ({ page }) => {
  const problems = watchPage(page);
  await boot(page);
  await settleBackground(page);

  await page.evaluate(() => (window as any).__plumbline.fetchQ("chordMap")); // MUTATION
  await openFromExplore(page, /^Weave map/);
  await openedMap(page, "Weave map");

  // ChordMap's own drawing contract (maps/ChordMap.svelte): a gold baseline
  // across the FULL width at y = H − 46, section bands and labels below it, the
  // OT/NT seam, and then ribbons arcing up from the axis. So: every column
  // carries ink (the axis spans the plot), the foot is busy (bands + labels),
  // and there is ink well above the axis (the ribbons) — 52 book pairs of it.
  // Measured on a correctly-painted map: 8.9% ink, 64/64 columns, 32% at the
  // foot, 6.0% in the ribbon band.
  const p = await drawnProfile(
    page,
    {
      ribbons: [0, 0.4, 1, 0.8],
      foot: [0, 0.8, 1, 1],
      oldTestament: [0, 0, 0.5, 1],
      newTestament: [0.5, 0, 1, 1],
    },
    problems,
  );
  expect(p.unpainted, `the weave map's canvas was never drawn on (${p.size})`).toBe(0);
  expect(p.columns, `the canon axis spans the whole width, so every column should carry ink (${p.size})`).toBe(COLUMNS);
  expect(p.ink, "the weave map should be substantially drawn").toBeGreaterThan(0.02);
  expect(p.bands.ribbons, "ribbons should arc above the axis").toBeGreaterThan(0.01);
  expect(p.bands.foot, "the canon axis, its section bands and their labels sit at the foot").toBeGreaterThan(0.05);
  // Both testaments, because a map that folded every pair into one half would
  // still pass a whole-canvas total.
  expect(p.bands.oldTestament, "the OT half should be drawn").toBeGreaterThan(0.02);
  expect(p.bands.newTestament, "the NT half should be drawn").toBeGreaterThan(0.02);

  expect(distinct(problems), "the weave map should not log anything broken").toEqual([]);
});

test("a tap on the weave map opens that book", async ({ page }) => {
  await boot(page);
  await settleBackground(page);

  // The hit test is x → book index → the active pane, and it is the reason the
  // map is worth having: it is a way INTO the text. Tap the far left and the far
  // right of the canon axis; the first book and the last book are the only two
  // answers that prove the mapping is not degenerate.
  await openFromExplore(page, /^Weave map/);
  await openedMap(page, "Weave map");
  let box = (await mapCanvas(page).boundingBox())!;
  await page.mouse.click(box.x + 4, box.y + box.height * 0.6);
  await expect(page.locator(".popup")).toHaveCount(0);
  expect(await pane(page)).toMatchObject({ book: "Gen", chapter: 1 });
  // Tapping a book takes the reader to the text, not back to Explore.
  expect(await page.evaluate(() => (window as any).__plumbline.screen)).toBe("read");

  await openFromExplore(page, /^Weave map/);
  await openedMap(page, "Weave map");
  box = (await mapCanvas(page).boundingBox())!;
  await page.mouse.click(box.x + box.width - 4, box.y + box.height * 0.6);
  await expect(page.locator(".popup")).toHaveCount(0);
  expect(await pane(page)).toMatchObject({ book: "Rev", chapter: 1 });
});

test("the constellation draws its lanes, names a star, pages and pins", async ({ page }) => {
  const problems = watchPage(page);
  await boot(page);
  await settleBackground(page);

  await openFromExplore(page, /^Constellation/);
  await openedMap(page, "Constellation");

  // Constellation's contract (maps/Constellation.svelte): a pin marker and the
  // lane's name in the left gutter (x < 150), then that lane's edges and nodes
  // across the plot (x ≥ 162). Both halves have to be drawn — a map with lanes
  // but no names is unreadable, and names with no lanes is empty paper.
  // Measured: 8.5% ink, 64/64 columns, gutter 6.7%, plot 8.7%.
  const p = await drawnProfile(page, { gutter: [0, 0, 0.125, 1], plot: [0.135, 0, 1, 1] }, problems);
  expect(p.unpainted, `the constellation's canvas was never drawn on (${p.size})`).toBe(0);
  expect(p.columns, `stars run the width of the canon, so every column should carry ink (${p.size})`).toBe(COLUMNS);
  expect(p.bands.gutter, "lane names and pin markers are drawn in the left gutter").toBeGreaterThan(0.01);
  expect(p.bands.plot, "the lanes themselves are drawn across the plot").toBeGreaterThan(0.01);

  // ── the star knows which verse it is ──
  // This is the assertion that cannot be faked: `nodePos` is filled by paint()
  // and by nothing else, so a hover that comes back with the right verse in the
  // right lane proves the map really placed that node where it says it did. The
  // geometry below is Constellation's own (plotLeft 162, topPad 18, lane height
  // (H − topPad − 10) / laneCapacity) — the pixel constants the file documents as
  // shared with the retired GTK/WinUI shells so every shell places a node alike.
  const star = await page.evaluate(() => {
    const m = (window as any).__plumbline.q("constellation", 0, []);
    const W = 1200;
    const H = 640;
    const PLOT_LEFT = 162;
    const TOP_PAD = 18;
    const laneH = (H - TOP_PAD - 10) / m.laneCapacity;
    let best: any = null;
    m.lanes.forEach((lane: any, li: number) => {
      for (const nd of lane.nodes ?? []) if (!best || nd.size > best.node.size) best = { node: nd, lane, li };
    });
    return {
      x: PLOT_LEFT + best.node.x * (W - PLOT_LEFT),
      y: TOP_PAD + (best.li + best.node.laneFrac) * laneH,
      gutterY: TOP_PAD + 0.5 * laneH,
      display: best.node.display,
      lane: best.lane.name,
      book: best.node.book,
      chapter: best.node.chapter,
      verse: best.node.verse,
      firstLane: m.lanes[0].name,
      W,
      H,
    };
  });

  const caption = page.locator(".popup .bar .caption");
  const resting = (await caption.textContent()) ?? "";
  expect(resting, "the frame's caption carries the engine's paging summary").toMatch(/weaves 1–\d+ of \d+/);

  await page.mouse.move(...(await pointAt(page, star.x, star.y, star.W, star.H)));
  await expect(caption, `hovering ${star.display} should name it`).toHaveText(`${star.display} · ${star.lane}`);
  expect(
    await mapCanvas(page).evaluate((c) => c.style.cursor),
    "a hit node should offer a pointer",
  ).toBe("pointer");

  // Off the node again: the caption goes back to what the engine said.
  const box = (await mapCanvas(page).boundingBox())!;
  await page.mouse.move(box.x + box.width - 5, box.y + box.height - 5);
  await expect(caption).toHaveText(resting);

  // ── tapping a star goes to that verse, and the map stays open ──
  // Deliberate, and documented in the component: a node navigates without
  // closing, so a reader can walk a lane verse by verse.
  await page.mouse.click(...(await pointAt(page, star.x, star.y, star.W, star.H)));
  expect(await pane(page)).toMatchObject({
    book: star.book,
    chapter: star.chapter,
    targetVerse: star.verse ?? null,
  });
  await expect(mapCanvas(page), "a star navigates without closing the map").toBeVisible();

  // ── the pager ──
  // `.pager > span`, not `.pager`: the pager element also holds its two arrow
  // buttons, so its own text is "‹ 1 / 2 ›". The count lives in the one child
  // span (MapFrame.svelte:58) — select the thing being asserted, not its box.
  const pageCount = page.locator(".popup .pager > span");
  await expect(pageCount).toHaveText(/^1 \/ \d+$/);
  await page.locator('.popup .pager button[aria-label="Next page"]').click();
  await expect(pageCount).toHaveText(/^2 \/ \d+$/);
  // A second page of a 28-weave library, and it draws: page 2 is smaller than
  // page 1 (28 weaves, 18 lanes to a page), so this is about it being drawn at
  // all rather than about how much.
  await expect(caption).toHaveText(/weaves 19–\d+ of \d+/);
  const p2 = await drawnProfile(page, { plot: [0.135, 0, 1, 1] });
  expect(p2.columns, "page two should be drawn across the canon too").toBe(COLUMNS);
  expect(p2.bands.plot, "page two should have lanes on it").toBeGreaterThan(0.005);

  // ── pinning a lane ──
  // The pin gutter is the leftmost 150 model px; the marker for lane 0 sits at
  // its middle. The caption is the engine's, so a caption that counts the pin is
  // proof the pins reached the worker and came back — which is the exact path
  // the DataCloneError above broke.
  await page.locator('.popup .pager button[aria-label="Previous page"]').click();
  await expect(pageCount).toHaveText(/^1 \/ \d+$/);
  await page.mouse.click(...(await pointAt(page, 14, star.gutterY, star.W, star.H)));
  await expect(caption, "pinning a lane should be counted in the caption").toHaveText(/^1 pinned · /);

  expect(distinct(problems), "the constellation should not log anything broken").toEqual([]);
});

test("every way out of a map leaves the reader exactly where it was", async ({ page }) => {
  await boot(page);
  await settleBackground(page);

  // Raised through session state, one at a time, because what is under test is
  // the way OUT — the routes in are covered above. The reader is behind these
  // (rather than the Explore screen, which replaces it), which is the case that
  // matters: dismissing a popup must not disturb the chapter underneath.
  //
  // THREE ways out over two maps, deliberately. The backdrop case used to ride on
  // the concept map, which was removed 2026-07-30; the way out is a property of
  // MapFrame rather than of any one map, so it moved onto the weave map rather
  // than going with it.
  const before = await pane(page);
  const ways: { name: string; open: string; out: (p: Page) => Promise<void> }[] = [
    {
      name: "chord ✕",
      open: `s.mapPopup = { kind: "chord" }`,
      out: async (p) => p.locator('.popup .bar button[aria-label="Close"]').click(),
    },
    {
      name: "constellation Escape",
      open: `s.mapPopup = { kind: "constellation" }`,
      out: async (p) => p.keyboard.press("Escape"),
    },
    {
      name: "chord backdrop",
      open: `s.mapPopup = { kind: "chord" }`,
      // `.backdrop` is shared by ten sheets, so insist there is exactly one on
      // screen before using it — with two surfaces open this would otherwise
      // dismiss whichever happened to be first in the DOM. Clicked beside the
      // popup rather than at the top-left corner: the header stacks ABOVE the
      // backdrop, so that corner is not the backdrop's to give.
      out: async (p) => {
        await expect(p.locator(".backdrop")).toHaveCount(1);
        const popup = (await p.locator(".popup").boundingBox())!;
        await p.mouse.click(popup.x / 2, popup.y + popup.height / 2);
      },
    },
  ];

  for (const way of ways) {
    await page.evaluate(`(() => { const s = window.__plumbline; ${way.open}; })()`);
    await expect(mapCanvas(page), `${way.name} should open`).toBeVisible({ timeout: 30_000 });
    await way.out(page);
    await expect(page.locator(".popup"), `${way.name} should close`).toHaveCount(0);
    expect(await page.evaluate(() => (window as any).__plumbline.mapPopup), `${way.name} should be forgotten`).toBeNull();
    await expect(page.locator(".pane canvas").first(), "the reader should still be there").toBeVisible();
    expect(await pane(page), `${way.name} should not have moved the reader`).toMatchObject({
      book: before.book,
      chapter: before.chapter,
    });
  }

  // And the reader still answers — a map that left a modal hand on the keyboard
  // would show up here and nowhere else.
  await page.keyboard.press("]");
  expect((await pane(page)).chapter, "the reader should still take keys").toBe(before.chapter + 1);
});

/** The active pane's state. */
async function pane(page: Page): Promise<{ book: string; chapter: number; targetVerse: number | null }> {
  return page.evaluate(() => {
    const s = (window as any).__plumbline;
    const p = s.panes[s.activePane];
    return { book: p.book, chapter: p.chapter, targetVerse: p.targetVerse };
  });
}
