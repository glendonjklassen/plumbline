import { expect, test, type Page } from "@playwright/test";

// The reader's frame path, asserted by counting what it does rather than by timing it.
//
// A chapter's display list is the biggest object the shell holds (2,643 items on Psalm 119) and is
// fixed the moment the core has laid the chapter out. Two things treated it as though it were not:
// it lived in deep `$state`, so Svelte built a proxy per item and a scroll frame walks the list
// three times; and `paintChapter` rebuilt `verseExtents` inside the frame, recomputing the
// per-verse span map from all 2,643 items on every scroll frame.
//
// So these count the two things themselves, out of `__plumblinePaint` (the handle
// `src/reader/paint.ts` publishes): how many items are reactive proxies, and how many times the
// extents were computed across a real scroll. The budget for the second is derived from the number
// of layouts the page reported in the same window — not a constant, and not a millisecond ceiling
// that a wholly un-memoized scroll could still fit inside.
//
// Psalm 119 because it is the longest chapter in the canon and the worst case for all of this.
//
// The middle test is the correctness half: performance work must not move a pixel, and a memo can
// only do that by going stale, so the extents the paint used are re-derived independently after a
// zoom has replaced the display list.
//
// Coupled to a11y.spec.ts's "scrolling does not rebuild the mirror": the mirror is derived from
// `items` alone so it stays off the scroll path, and raw state is a signal on the variable, so a
// new display list invalidates it and nothing else does.

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

/** Psalm 119, laid out and painted. 176 mirror paragraphs means the list is in hand, and
 *  `__plumblinePaint.items` is only written when a new list is painted, so a long list there means
 *  the counters below start from a warm memo — the state a reader scrolls in. */
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

/** A real wheel scroll over the text, one notch at a time, waiting for the frame each notch
 *  causes. Wheel rather than assigning `scrollTop`, so the whole scroll → `pane.scrollY` → effect
 *  → rAF paint chain runs the way a reader's thumb runs it. */
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

// Fails against the bug: with `verseExtents` skipping its memo, extentsComputed tracks the frame
// count (10 computations across 10 paints) against a ceiling of `layouts`, which is 0.
test("scrolling Psalm 119 does not recompute the verse extents", async ({ page }) => {
  await boot(page);
  await psalm119(page);

  await page.evaluate(() => (window as any).__plumblinePaint.reset());
  await scrollSteps(page, 8);
  const c = await counters(page);

  // The pane really scrolled and painted; without this the counts below could all be zero.
  expect(await page.locator(".pane .scroll").evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  expect(c.paints, "scrolling painted no frames").toBeGreaterThanOrEqual(8);
  expect(c.extentsCalls, "the frame path did not ask for the verse extents at all").toBeGreaterThanOrEqual(
    c.paints,
  );
  // Once per layout, not once per frame. The ceiling is the number of display lists the page
  // reported in this same window, so nothing here is a constant.
  const recomputed =
    `verseExtents was recomputed on the frame path: ${c.extentsComputed} computations ` +
    `across ${c.paints} paints of ${c.layouts} new layout(s)`;
  expect(c.extentsComputed, recomputed).toBeLessThanOrEqual(c.layouts);
  // The ceiling only means something because many frames shared one layout.
  expect(c.paints, "not enough frames shared a layout for the budget to mean anything").toBeGreaterThan(
    c.layouts + 1,
  );
});

// A memo can only change the painted result by going stale. So: replace the display list with a
// geometrically different one (a zoom re-lays the same 2,643 items at new y positions — same
// length, so a memo keyed on anything weaker than identity hands back the old map), re-derive the
// extents here and compare against the ones the paint used. Fails against the bug: keyed on
// `items.length`, all 176 verses disagree.
test("the memoized verse extents are the extents of the chapter on screen", async ({ page }) => {
  await boot(page);
  await psalm119(page);

  const before = await counters(page);
  // Derived from the reader's own body size, so it cannot become a no-op against a profile that
  // already reads at that size.
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
    // Re-derived here rather than by calling the shipped function: this checks its answer.
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

// Fails against the bug: with `items` back in deep `$state` in ReaderPane.svelte, all 2,643 items
// report as reactive proxies, plus the list itself.
test("the display list is not deep reactive state", async ({ page }) => {
  await boot(page);
  await psalm119(page);

  const r = await page.evaluate(() => {
    // A Svelte deep-state proxy refuses `setPrototypeOf` (`state_prototype_fixed`), while for an
    // ordinary object setting the prototype it already has succeeds as a no-op
    // (OrdinarySetPrototypeOf returns early on SameValue). So this asks "is a handler in the way?"
    // and changes nothing.
    const refuses = (o: unknown): boolean => {
      try {
        Object.setPrototypeOf(o as object, Object.getPrototypeOf(o as object));
        return false;
      } catch {
        return true;
      }
    };
    // Controls for the detector in this same realm, so one that had become "always false" cannot
    // make the count below vacuous.
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
  // Psalm 119: 2,467 words + 176 verse numbers, asserted loosely — what matters is that this is
  // the long chapter, not the exact tokenization.
  expect(r.total, "this is not Psalm 119's display list").toBeGreaterThan(2000);
  expect(r.fields, "a display-list item is more than a couple of fields").toBeGreaterThan(5);
  const deep =
    `the display list is deep reactive state: ${r.reactive} of ${r.total} items are reactive proxies` +
    (r.list ? ", plus the list itself" : "");
  expect(r.reactive, deep).toBe(0);
  expect(r.list, "the display list array is itself a reactive proxy").toBe(false);
});
