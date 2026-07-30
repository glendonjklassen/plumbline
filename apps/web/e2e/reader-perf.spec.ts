import { expect, test, type Page } from "@playwright/test";

// The reader's FRAME PATH, asserted by counting what it does rather than by
// timing how long it took.
//
// A chapter's display list is the biggest object the shell holds — 2,643 items
// on Psalm 119 — and every one of them is fixed the moment the core has laid the
// chapter out. Two things used to treat it as though it were not:
//
//   * it was held in deep `$state`, so Svelte built a proxy per item — 2,643 of
//     them, and the count is measured below, not estimated — with a signal behind
//     each field those proxies were read for, all to watch for changes that cannot
//     happen. A scroll frame walks the list three times, so every read went
//     through a trap;
//   * `paintChapter` rebuilt `verseExtents` INSIDE the frame, so the per-verse
//     span map was recomputed from all 2,643 items on every scroll frame, from
//     data that changes only when the layout does.
//
// So the tests below measure the two things themselves: how many of the items are
// reactive proxies, and how many times the extents were computed across a real
// scroll. Both are counts read out of the page (`__plumblinePaint`, the handle
// `src/reader/paint.ts` publishes), and the budget for the second is DERIVED from
// the number of layouts the page reported during the same window — not a constant,
// and not a millisecond ceiling. The working rules record two tests that passed
// against the bug they described, one of them for exactly that reason.
//
// Psalm 119 on purpose: the longest chapter in the canon, 176 verses, and the
// case where all of this is worst.
//
// COUPLED TO a11y.spec.ts's "scrolling does not rebuild the mirror". The text
// mirror is derived from `items` alone precisely so it stays off the scroll path,
// and raw state is the reason that still holds: raw state is a signal on the
// VARIABLE, so the mirror is invalidated by a new display list and by nothing
// else. Run the two files together — that test is the other half of this one's
// equivalence claim, and it passes with this change in.
//
// PERFORMANCE, so the painted result must be unchanged, and the middle test is
// that proof: the extents the paint actually used are re-derived independently
// here, AFTER a zoom has replaced the display list, and must equal the extents of
// the list on screen. A memo can only change a pixel by going stale, and that is
// the assertion that it has not.
//
// The pixels were also checked directly, once, outside these tests (2026-07-30):
// ten SHA-256 fingerprints of `canvas.toDataURL()` — Genesis 1, Psalm 119 and
// John 3 each at scroll 0 / 400 / 1200, plus Psalm 119 navigated to verse 40 so
// the goto band and the extents-driven gutter marks are in the frame — captured
// with the change in and with both halves of it backed out. All ten hashes are
// identical. That harness is not kept here because of what it took to make it
// honest: the reader sets its text in "EB Garamond", a WEB font, and a canvas
// sampled before the font arrives is a picture of Georgia — a first pass compared
// a cold run against a warm one and "found" ten differences that were entirely
// the fallback face. Fingerprinting this canvas means waiting for the font and
// then sampling until two consecutive samples agree.
//
// Mutation-tested 2026-07-30 (working rules: break the fix, watch it fail,
// restore) — the exact failures are recorded above each test.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  await expect(page.locator('.pane .mirror p[data-verse="16"]')).toHaveCount(1, { timeout: 90_000 });
}

/** Psalm 119, laid out and painted. The mirror is derived from the display list,
 *  so 176 paragraphs means the list is in hand; `__plumblinePaint.items` is only
 *  written when a NEW list is painted, so a long list there means the frame path
 *  has seen it and the counters below start from a warm memo — which is the state
 *  a reader scrolls in. */
async function psalm119(page: Page): Promise<void> {
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Ps", 119));
  await expect(page.locator(".pane .mirror p")).toHaveCount(176, { timeout: 90_000 });
  await expect
    .poll(async () => page.evaluate(() => (window as any).__plumblinePaint.items?.deref()?.length ?? 0), {
      timeout: 30_000,
      message: "Psalm 119's display list never reached the painter",
    })
    .toBeGreaterThan(2000);
}

interface Counters {
  paints: number;
  layouts: number;
  extentsCalls: number;
  extentsComputed: number;
  items: number;
}

async function counters(page: Page): Promise<Counters> {
  return await page.evaluate(() => {
    const p = (window as any).__plumblinePaint;
    return {
      paints: p.paints,
      layouts: p.layouts,
      extentsCalls: p.extentsCalls,
      extentsComputed: p.extentsComputed,
      items: p.items?.deref()?.length ?? 0,
    };
  });
}

/** A real wheel scroll over the text, one notch at a time, waiting for the frame
 *  each notch causes. Wheel rather than assigning `scrollTop`: the container
 *  scrolls natively and the whole scroll → `pane.scrollY` → effect → rAF paint
 *  chain runs the way a reader's thumb runs it. */
async function scrollSteps(page: Page, steps: number): Promise<void> {
  const box = (await page.locator(".pane .scroll").boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  for (let i = 0; i < steps; i++) {
    const before = (await counters(page)).paints;
    await page.mouse.wheel(0, 320);
    await expect
      .poll(async () => (await counters(page)).paints, {
        timeout: 10_000,
        message: `scroll step ${i + 1} never reached the painter`,
      })
      .toBeGreaterThan(before);
  }
}

// Mutation: `verseExtents` made to skip the memo (`const memo = undefined`) —
//   i.e. the shipped behaviour before this change →
//   'Error: verseExtents was recomputed on the frame path: 10 computations across
//    10 paints of 0 new layout(s)  expect(received).toBeLessThanOrEqual(expected)
//    Expected: <= 0  Received: 10'
//   (the two counts move together and track how many frames the wheel produced,
//   which is the point: the ceiling is `layouts`, and it was 0.)
test("scrolling Psalm 119 does not recompute the verse extents", async ({ page }) => {
  await boot(page);
  await psalm119(page);

  await page.evaluate(() => (window as any).__plumblinePaint.reset());
  await scrollSteps(page, 8);
  const c = await counters(page);

  // The pane really scrolled and really painted — without this the counts below
  // could all be zero and mean nothing.
  expect(await page.locator(".pane .scroll").evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  expect(c.paints, "scrolling painted no frames").toBeGreaterThanOrEqual(8);
  expect(c.extentsCalls, "the frame path did not ask for the verse extents at all").toBeGreaterThanOrEqual(
    c.paints,
  );
  // The whole claim: ONCE PER LAYOUT, not once per frame. The ceiling is the
  // number of display lists the page reported in this same window (0, unless
  // something re-laid the chapter mid-scroll), so nothing here is a constant.
  const recomputed =
    `verseExtents was recomputed on the frame path: ${c.extentsComputed} computations ` +
    `across ${c.paints} paints of ${c.layouts} new layout(s)`;
  expect(c.extentsComputed, recomputed).toBeLessThanOrEqual(c.layouts);
  // ...and the ceiling is only meaningful because many frames shared one layout.
  expect(c.paints, "not enough frames shared a layout for the budget to mean anything").toBeGreaterThan(
    c.layouts + 1,
  );
});

// The painted result must be identical, and a memo can only change it by going
// stale. So: replace the display list with a geometrically DIFFERENT one (a zoom
// re-lays out the same 2,643 items at new y positions — same length, so a memo
// keyed on anything weaker than identity would hand back the old map), then
// re-derive the extents here and compare against the ones the paint used.
//
// Mutation: memo keyed on `items.length` instead of on the list itself
//   (`extentsMemo` → `Map<number, VerseExtents>` keyed by length) →
//   'Error: the extents the paint used are not the extents of the list on
//    screen: 176 of 176 verses disagree (first: 1, 2, 3, 4)
//    expect(received).toBe(expected)  Expected: 0  Received: 176'
test("the memoized verse extents are the extents of the chapter on screen", async ({ page }) => {
  await boot(page);
  await psalm119(page);

  const before = await counters(page);
  // Derived from the reader's own body size rather than hardcoded, so this cannot
  // quietly become a no-op against a profile that already reads at that size.
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.setZoom(Number(s.config.bodySize ?? 18) + 5);
  });
  await expect
    .poll(async () => (await counters(page)).layouts, {
      timeout: 30_000,
      message: "the zoom never produced a new display list",
    })
    .toBeGreaterThan(before.layouts);

  const cmp = await page.evaluate(() => {
    const p = (window as any).__plumblinePaint;
    const items = p.items.deref();
    const used = p.extents as Map<number, { top: number; bottom: number; firstY: number }>;
    // Re-derived here, deliberately NOT by calling the shipped function: the
    // point is to check its answer, not to run it twice.
    const fresh = new Map<number, { top: number; bottom: number; firstY: number }>();
    for (const it of items) {
      const v =
        it.verseNumber !== null
          ? it.verseNumber
          : it.verse
            ? Number(it.verse.slice(it.verse.lastIndexOf(":") + 1)) || null
            : null;
      if (v === null) continue;
      const e = fresh.get(v);
      if (!e) fresh.set(v, { top: it.y, bottom: it.y + it.h, firstY: it.y });
      else {
        e.top = Math.min(e.top, it.y);
        e.bottom = Math.max(e.bottom, it.y + it.h);
        e.firstY = Math.min(e.firstY, it.y);
      }
    }
    const disagree: number[] = [];
    for (const [v, f] of fresh) {
      const u = used.get(v);
      if (!u || u.top !== f.top || u.bottom !== f.bottom || u.firstY !== f.firstY) disagree.push(v);
    }
    return { verses: fresh.size, used: used.size, disagree: disagree.length, first: disagree.slice(0, 4) };
  });

  expect(cmp.verses, "Psalm 119 is 176 verses").toBe(176);
  expect(cmp.used, "the paint had extents for a different number of verses").toBe(cmp.verses);
  const stale =
    `the extents the paint used are not the extents of the list on screen: ` +
    `${cmp.disagree} of ${cmp.verses} verses disagree` +
    (cmp.first.length ? ` (first: ${cmp.first.join(", ")})` : "");
  expect(cmp.disagree, stale).toBe(0);
});

// Mutation: `items` back to deep `$state` in ReaderPane.svelte →
//   'Error: the display list is deep reactive state: 2643 of 2643 items are
//    reactive proxies, plus the list itself  expect(received).toBe(expected)
//    Expected: 0  Received: 2643'
test("the display list is not deep reactive state", async ({ page }) => {
  await boot(page);
  await psalm119(page);

  const r = await page.evaluate(() => {
    // A Svelte deep-state proxy REFUSES `setPrototypeOf` — its handler throws
    // `state_prototype_fixed`. For any ordinary object, setting the prototype it
    // already has is a no-op that succeeds (OrdinarySetPrototypeOf returns early
    // on SameValue), so this asks "is a handler in the way?" and changes nothing.
    const refuses = (o: unknown): boolean => {
      try {
        Object.setPrototypeOf(o as object, Object.getPrototypeOf(o as object));
        return false;
      } catch {
        return true;
      }
    };
    // Controls for the detector itself, in this same realm — so a helper that
    // had quietly become "always false" could not make the count below vacuous.
    const control = {
      plain: refuses({}),
      guarded: refuses(
        new Proxy(
          {},
          {
            setPrototypeOf() {
              throw new Error("fixed");
            },
          },
        ),
      ),
    };
    const items = (window as any).__plumblinePaint.items.deref();
    let reactive = 0;
    for (const it of items) if (refuses(it)) reactive++;
    return {
      control,
      total: items.length,
      reactive,
      list: refuses(items),
      fields: Object.keys(items[0] ?? {}).length,
    };
  });

  expect(r.control.plain, "the proxy detector reports a plain object as reactive").toBe(false);
  expect(r.control.guarded, "the proxy detector cannot see a proxy at all").toBe(true);
  // Psalm 119: 2,467 words + 176 verse numbers. Asserted loosely, because the
  // point is that this is the LONG chapter, not the exact tokenization.
  expect(r.total, "this is not Psalm 119's display list").toBeGreaterThan(2000);
  expect(r.fields, "a display-list item is more than a couple of fields").toBeGreaterThan(5);
  const deep =
    `the display list is deep reactive state: ${r.reactive} of ${r.total} items are reactive proxies` +
    (r.list ? ", plus the list itself" : "");
  expect(r.reactive, deep).toBe(0);
  expect(r.list, "the display list array is itself a reactive proxy").toBe(false);
});
