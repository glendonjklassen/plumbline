// The ambient weave connectors are drawn on one canvas spanning the whole pane
// row, and the effect that draws them depends on every pane's scrollY — so it
// runs on EVERY scroll frame. It used to open that draw with an unconditional
//
//     canvas.width  = Math.round(cssW * dpr);
//     canvas.height = Math.round(cssH * dpr);
//
// and assigning either of those REALLOCATES the backing store and clears it even
// when the number assigned is the number already there. On a phone there is one
// pane, so there is no cross-pane connector in existence to draw: the whole cost
// was a full-viewport allocation per frame to paint nothing at all.
//
// So this counts the ALLOCATIONS THEMSELVES rather than a proxy for them. The
// HTMLCanvasElement width/height setters are patched in the page, and every write
// is recorded with which canvas took it and whether the value written was already
// there. A redundant write is not "a bit of waste" — it is exactly the bug.
//
// Three states, because the fix has three parts and each one owns a state: one
// pane (the overlay is not mounted), two panes with a weave link across them (it
// draws every frame and sizes its canvas once), and two panes with nothing woven
// between them (it is mounted, the effect runs, and the frame must cost nothing).
//
// There is no time budget anywhere below, deliberately. The assertions are "zero"
// and "did not grow when the frame count doubled": both are exact, and neither can
// pass on a fast machine by luck. (A fixed-millisecond ceiling once passed against
// the very bug it described — see the working rules.)
//
// e2e/connectors.spec.ts is the other half of this pair and the equivalence proof:
// it asserts the connectors still MEET their verses, at two chrome heights. This
// file only asserts what the frame costs.
import { expect, test, type Page } from "@playwright/test";

interface Counts {
  /** width/height writes on a canvas inside the connectors overlay. */
  overlaySize: number;
  /** ...of those, writes whose value was already there: pure reallocation. */
  overlayRedundant: number;
  /** clearRect calls on the overlay's context — one per frame that really drew. */
  overlayFrames: number;
  /** setTransform calls on a reader pane's canvas — one per pane repaint. The
   *  witness that the scroll actually drove frames, which matters most in the
   *  one-pane test where the overlay is not there to leave a trace. */
  paneFrames: number;
}

/** Patch the canvas size setters before any app code runs. Reports per-canvas so
 *  the reader panes (which have always guarded their own resize) cannot be
 *  mistaken for the overlay. */
async function instrument(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const c: Counts = { overlaySize: 0, overlayRedundant: 0, overlayFrames: 0, paneFrames: 0 };
    (window as any).__cvx = {
      read: () => ({ ...c }),
      reset: () => {
        c.overlaySize = 0;
        c.overlayRedundant = 0;
        c.overlayFrames = 0;
        c.paneFrames = 0;
      },
    };
    const isOverlay = (el: HTMLCanvasElement | null) => !!el?.isConnected && !!el.closest(".overlay");
    const isPane = (el: HTMLCanvasElement | null) => !!el?.isConnected && !!el.closest(".pane");

    for (const dim of ["width", "height"] as const) {
      const d = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, dim)!;
      Object.defineProperty(HTMLCanvasElement.prototype, dim, {
        configurable: true,
        enumerable: d.enumerable,
        get(this: HTMLCanvasElement) {
          return d.get!.call(this);
        },
        set(this: HTMLCanvasElement, v: number) {
          if (isOverlay(this)) {
            c.overlaySize++;
            // The value already there. Writing it anyway is the reallocation.
            if (d.get!.call(this) === v) c.overlayRedundant++;
          }
          d.set!.call(this, v);
        },
      });
    }

    const proto = CanvasRenderingContext2D.prototype;
    const clearRect = proto.clearRect;
    proto.clearRect = function (this: CanvasRenderingContext2D, ...a: [number, number, number, number]) {
      if (isOverlay(this.canvas)) c.overlayFrames++;
      return clearRect.apply(this, a);
    };
    const setTransform = proto.setTransform;
    proto.setTransform = function (this: CanvasRenderingContext2D, ...a: any[]) {
      if (isPane(this.canvas)) c.paneFrames++;
      return (setTransform as any).apply(this, a);
    };
  });
}

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

interface Pair {
  aBook: string;
  aChapter: number;
  bBook: string;
  bChapter: number;
}

/** Open the two chapters of one stock weave link side by side, and specifically
 *  one whose LEFT chapter is longer than the pane — a chapter that fits has no
 *  scroll range, `scrollTop` never moves, and the scroll frames this whole file
 *  measures never happen. (The first cross-chapter pair in the stock set is Ps
 *  110 ↔ Heb 5, and Psalm 110 fits: the first attempt at this test measured 1
 *  draw in 30 frames and looked like a broken overlay.) The link is taken from
 *  the engine's own pairs rather than named here, so it does not rot when the
 *  stock set changes. */
async function openScrollableWovenPair(page: Page): Promise<void> {
  const candidates: Pair[] = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const pairs: any[] = ((await s.fetchQ("linkPairs"))?.pairs ?? []).filter((p: any) => p.resolved);
    if (s.panes.length < 2) s.addPane(0);
    return pairs
      .filter((p) => !(p.aBook === p.bBook && p.aChapter === p.bChapter))
      .map((p) => ({ aBook: p.aBook, aChapter: p.aChapter, bBook: p.bBook, bChapter: p.bChapter }));
  });
  expect(
    candidates.length,
    "the stock weaves should hold at least one cross-chapter link",
  ).toBeGreaterThan(0);
  expect(
    await page.evaluate(() => (window as any).__plumbline.panes.length),
    "two panes are the whole point of this test",
  ).toBe(2);

  const tried: string[] = [];
  for (const c of candidates.slice(0, 12)) {
    await page.evaluate((c: Pair) => {
      const s = (window as any).__plumbline;
      s.navigate(0, c.aBook, c.aChapter);
      s.navigate(1, c.bBook, c.bChapter);
    }, c);
    // Both panes laid out, at the chapters just asked for.
    await page.waitForFunction(
      (c: Pair) => {
        const s = (window as any).__plumbline;
        return (
          s.panes[0]?.book === c.aBook &&
          s.panes[0]?.chapter === c.aChapter &&
          s.panes[1]?.book === c.bBook &&
          s.panes[1]?.chapter === c.bChapter &&
          (s.paneVerseGeom[0]?.size ?? 0) > 0 &&
          (s.paneVerseGeom[1]?.size ?? 0) > 0
        );
      },
      c,
      { timeout: 60_000 },
    );
    const range = await page.evaluate(() => {
      const port = document.querySelector<HTMLElement>(".pane .scroll")!;
      return port.scrollHeight - port.clientHeight;
    });
    tried.push(`${c.aBook} ${c.aChapter} (${range}px)`);
    if (range > 300) return;
  }
  throw new Error(`no woven pair put a scrollable chapter in pane 0; tried ${tried.join(", ")}`);
}

interface Unwoven {
  p0: string;
  p1: string;
  /** Pairs crossing the two panes as they FINALLY stand — the number the draw
   *  itself computes, recounted after the navigation settled rather than assumed
   *  from the chapter that was asked for. */
  crossing: number;
}

/** Leave pane 0 on the scrollable chapter it already holds and send pane 1 to a
 *  chapter NOTHING in the weaves connects to it — the commonest two-pane state
 *  there is. The chapter is found by asking the engine's own pairs, so this does
 *  not rot when the stock set changes. */
async function sendPane1SomewhereUnwoven(page: Page): Promise<Unwoven> {
  const want = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const pairs: any[] = ((await s.fetchQ("linkPairs"))?.pairs ?? []).filter((p: any) => p.resolved);
    const a = { book: s.panes[0].book as string, chapter: s.panes[0].chapter as number };
    const woven = (book: string, chapter: number) =>
      pairs.some(
        (p) =>
          (p.aBook === a.book && p.aChapter === a.chapter && p.bBook === book && p.bChapter === chapter) ||
          (p.bBook === a.book && p.bChapter === a.chapter && p.aBook === book && p.aChapter === chapter),
      );
    for (const b of (s.q("toc")?.books ?? []) as any[])
      for (let c = 1; c <= (Number(b.chapters) || 0); c++) {
        if (b.id === a.book && c === a.chapter) continue;
        if (woven(b.id, c)) continue;
        s.navigate(1, b.id, c);
        return `${b.id} ${c}`;
      }
    return null;
  });
  expect(want, "no chapter in the canon is unwoven to pane 0's, which cannot be").not.toBeNull();
  await page.waitForFunction(
    (want: string) => {
      const s = (window as any).__plumbline;
      return `${s.panes[1]?.book} ${s.panes[1]?.chapter}` === want;
    },
    want as string,
    { timeout: 60_000 },
  );

  // Recount against the settled panes: the premise of the whole test is that this
  // configuration gives the draw nothing to do, and a navigation that landed
  // somewhere else must not pass for one that did.
  const where = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const pairs: any[] = ((await s.fetchQ("linkPairs"))?.pairs ?? []).filter((p: any) => p.resolved);
    const key = (i: number) => `${s.panes[i].book}|${s.panes[i].chapter}`;
    const paneFor = new Map<string, number>();
    s.panes.forEach((p: any, i: number) => paneFor.set(`${p.book}|${p.chapter}`, i));
    const crossing = pairs.filter((p) => {
      const ia = paneFor.get(`${p.aBook}|${p.aChapter}`);
      const ib = paneFor.get(`${p.bBook}|${p.bChapter}`);
      return ia !== undefined && ib !== undefined && ia !== ib;
    }).length;
    return { p0: key(0).replace("|", " "), p1: key(1).replace("|", " "), crossing };
  });
  expect(where.crossing, `${where.p0} and ${where.p1} are woven to each other after all`).toBe(0);
  return where;
}

/** Drive a real native scroll for `frames` animation frames, one small step per
 *  frame, and return once the last step's draw has had two frames to land. */
async function scrollFrames(page: Page, frames: number): Promise<void> {
  await page.evaluate(
    (frames: number) =>
      new Promise<void>((resolve, reject) => {
        const port = document.querySelector<HTMLElement>(".pane .scroll");
        if (!port) return reject(new Error("no pane scrollport to scroll"));
        let i = 0;
        let dir = 1;
        const step = () => {
          port.scrollTop += 3 * dir;
          const max = port.scrollHeight - port.clientHeight;
          if (port.scrollTop >= max - 3 || port.scrollTop <= 0) dir = -dir;
          if (++i >= frames) {
            requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
            return;
          }
          requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      }),
    frames,
  );
}

const counts = (page: Page) => page.evaluate(() => (window as any).__cvx.read() as Counts);
const reset = (page: Page) => page.evaluate(() => (window as any).__cvx.reset());

/** Long enough that a per-frame allocation is unmistakable, short enough to stay
 *  inside a chapter's scroll range at 3px a frame. */
const BURST = 30;

test("scrolling a one-pane reader allocates no connector canvas at all", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 }); // a phone: narrow, one pane
  await instrument(page);
  await boot(page);

  expect(
    await page.evaluate(() => (window as any).__plumbline.panes.length),
    "a narrow viewport must open exactly one pane, or this test is not testing a phone",
  ).toBe(1);

  await reset(page);
  await scrollFrames(page, BURST);
  const c = await counts(page);

  expect(
    c.paneFrames,
    "the scroll did not repaint the pane, so nothing here was measured",
  ).toBeGreaterThan(BURST / 3);
  expect(
    c.overlaySize,
    `the connectors overlay allocated a backing store ${c.overlaySize} times across ` +
      `${c.paneFrames} scroll repaints (${(c.overlaySize / Math.max(1, c.paneFrames)).toFixed(2)} ` +
      `per frame) on a ONE-PANE reader, where there is no cross-pane connector to draw. ` +
      `Assigning canvas.width/height reallocates and clears the whole buffer even when the ` +
      `value is unchanged: the draw must bail before it, not after.`,
  ).toBe(0);

  // And the strongest form of that: on one pane the overlay is not mounted, so
  // its canvas cannot cost anything, ever.
  expect(
    await page.locator(".panes .overlay canvas").count(),
    "a one-pane reader should not carry a connectors overlay at all",
  ).toBe(0);
});

test("two panes redraw their connectors every scroll frame without reallocating", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1400, height: 950 });
  await instrument(page);
  await boot(page);

  // Put the two chapters of one stock weave link side by side, chosen from the
  // engine's own pairs so this does not rot when the stock set changes.
  await openScrollableWovenPair(page);
  // Let the overlay mount, size its canvas once, and settle.
  await expect(page.locator(".panes .overlay canvas")).toHaveCount(1);
  await page.waitForFunction(() => (window as any).__cvx.read().overlayFrames > 0, undefined, {
    timeout: 30_000,
  });

  // Two bursts, the second twice the first. A per-frame allocation grows with the
  // frames; a size that is only written when it changes does not.
  await reset(page);
  await scrollFrames(page, BURST);
  const one = await counts(page);
  await reset(page);
  await scrollFrames(page, BURST * 2);
  const two = await counts(page);

  expect(
    two.overlayFrames,
    `the overlay must still redraw as the reader scrolls: it drew ${one.overlayFrames} frames ` +
      `in ${BURST} scroll steps and ${two.overlayFrames} in ${BURST * 2}`,
  ).toBeGreaterThan(one.overlayFrames);

  expect(
    two.overlaySize,
    `the overlay reallocated its backing store ${one.overlaySize} times over ${one.overlayFrames} ` +
      `draws and ${two.overlaySize} times over ${two.overlayFrames} — it GROWS WITH THE FRAMES, ` +
      `which is a full-viewport canvas thrown away and remade on every scroll frame to draw the ` +
      `same connectors. Guard the width/height assignment: writing the value already there ` +
      `reallocates and clears just the same.`,
  ).toBe(one.overlaySize);
  expect(
    one.overlaySize + two.overlaySize,
    `the canvas size was written ${one.overlaySize + two.overlaySize} times during a scroll that ` +
      `changed neither the viewport nor the device pixel ratio`,
  ).toBe(0);
  expect(
    one.overlayRedundant + two.overlayRedundant,
    "no canvas size write may assign the value that is already there — that is the reallocation",
  ).toBe(0);

  // The connectors are still on the canvas after all that: a "cheap" frame that
  // draws nothing would satisfy every count above.
  expect(
    await page.evaluate(() => {
      const c = document.querySelector<HTMLCanvasElement>(".panes .overlay canvas")!;
      const px = c.getContext("2d")!.getImageData(0, 0, c.width, c.height).data;
      let n = 0;
      for (let i = 3; i < px.length; i += 4) if (px[i] > 0) n++;
      return n;
    }),
    "the overlay canvas is empty: the connectors stopped being drawn",
  ).toBeGreaterThan(0);
});

test("two panes with nothing woven between them allocate no canvas either", async ({ page }) => {
  // The commonest two-pane state on a desk: the overlay IS mounted, the effect
  // still runs on every scroll frame, and there is not one connector to draw. The
  // one-pane test above cannot reach this path, because there the overlay is not
  // mounted at all — so without this the "bail before the allocation" half of the
  // fix is only proven for a phone.
  await page.setViewportSize({ width: 1400, height: 950 });
  await instrument(page);
  await boot(page);

  // Borrow the woven-pair setup for its scrollable left chapter, then break the
  // weave by moving the right pane somewhere unrelated.
  await openScrollableWovenPair(page);
  await expect(page.locator(".panes .overlay canvas")).toHaveCount(1);
  const where = await sendPane1SomewhereUnwoven(page);
  // A few frames BEFORE the counters are read, so the one legitimate erase — the
  // previous configuration's connectors being wiped off the canvas — is spent
  // outside the measurement instead of racing it.
  await scrollFrames(page, 4);

  await reset(page);
  await scrollFrames(page, BURST);
  const c = await counts(page);

  expect(
    c.paneFrames,
    "the scroll did not repaint the panes, so nothing here was measured",
  ).toBeGreaterThan(BURST / 3);
  expect(
    c.overlaySize,
    `with ${where.p0} beside ${where.p1} — no weave link crosses them — the overlay still ` +
      `allocated a backing store ${c.overlaySize} times across ${c.paneFrames} scroll repaints. ` +
      `A frame with nothing to draw must bail BEFORE the canvas size assignment, not after it: ` +
      `writing canvas.width reallocates and clears the whole buffer whatever it is given.`,
  ).toBe(0);
  expect(
    c.overlayFrames,
    `the overlay cleared its canvas ${c.overlayFrames} times over ${c.paneFrames} scroll repaints ` +
      `with nothing woven between the panes — the erase is the one thing on this path that costs, ` +
      `so it belongs to the frame that had ink, not to every frame after it`,
  ).toBe(0);
});
