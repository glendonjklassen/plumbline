import { expect, test, type Page } from "@playwright/test";

// THE ⛓ CHAIN (maintainer UAT, 2026-08-18): two panes on the SAME chapter —
// the motivating case is two languages — gain a chain toggle in the pane
// strip, and while it is on they scroll TOGETHER, verse-aligned. Verse-aligned
// and not offset-copied: the same chapter in two languages runs to different
// heights, so a copied scrollTop drifts further apart the deeper you read.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** Split via the strip's real ＋, then wait for BOTH panes' verse geometry —
 *  the sync has nothing to align until the partner has laid out. */
async function splitPane(page: Page): Promise<void> {
  await page.locator(".pane").first().locator('button[title="Split pane"]').click();
  await expect(page.locator(".pane")).toHaveCount(2);
  await expect
    .poll(
      () =>
        page.evaluate(() => {
          const s = (window as any).__plumbline;
          return Math.min(s.paneVerseGeom[0]?.size ?? 0, s.paneVerseGeom[1]?.size ?? 0);
        }),
      { timeout: 60_000 },
    )
    .toBeGreaterThan(3);
}

const chain = (page: Page) => page.locator('button[title="Scroll together"]').first();

/**

 *  Scroll pane `idx`'s native container the way a reader does — the browser
 *  fires the scroll event; ReaderPane's user branch does the rest. */
async function scrollPane(page: Page, idx: number, top: number): Promise<void> {
  await page.evaluate(
    ([i, y]) => {
      const el = document.querySelectorAll(".pane .scroll")[i as number] as HTMLElement;
      el.scrollTop = y as number;
    },
    [idx, top],
  );
}

const paneTop = (page: Page, idx: number) =>
  page.evaluate((i) => (document.querySelectorAll(".pane .scroll")[i] as HTMLElement).scrollTop, idx);

// MUTATION: drop the `s.syncLinkedScroll(paneIdx)` call from ReaderPane's
// user-scroll branch (and rebuild). Red: the partner never moves.
test("chained panes scroll together, and unchained panes do not", async ({ page }) => {
  await boot(page);
  await splitPane(page);

  // No chain shown? The panes must share a chapter for it to appear at all.
  await expect(chain(page)).toBeVisible();

  // UNCHAINED first: the partner must hold still. The default is off — a
  // second pane is usually a reference being consulted, not a mirror.
  await scrollPane(page, 0, 400);
  await page.waitForTimeout(250);
  expect(await paneTop(page, 1), "unchained, the partner holds still").toBe(0);

  await chain(page).click();
  await scrollPane(page, 0, 800);
  // Same chapter, same language, same width → identical geometry, so the
  // aligned target IS the same offset.
  await expect.poll(() => paneTop(page, 1), { timeout: 10_000 }).toBeGreaterThan(700);

  // And a chain has two ends: scrolling the PARTNER moves the first pane.
  await scrollPane(page, 1, 200);
  await expect.poll(() => paneTop(page, 0), { timeout: 10_000 }).toBeLessThan(400);

  // Unchain: back to independent columns.
  await chain(page).click();
  const held = await paneTop(page, 1);
  await scrollPane(page, 0, 1000);
  await page.waitForTimeout(250);
  expect(await paneTop(page, 1), "unchained again, the partner holds still").toBe(held);
});

// MUTATION: in session.syncLinkedScroll, replace the verse-aligned `target`
// with a raw `top` copy (and rebuild). Red: the German column lands at the
// English offset, not at its own line for the same verse.
test("a German pane aligns by VERSE, not by copied offset", async ({ page }) => {
  test.setTimeout(300_000); // the Luther pack downloads once, inside the test
  await boot(page);
  await splitPane(page);

  const pane1 = page.locator(".pane").nth(1);
  await pane1.locator("button.lang").click();
  await pane1.getByRole("menuitem").filter({ hasText: "Luther" }).click();
  await expect
    .poll(async () => await page.evaluate(() => (window as any).__plumbline.panes[1]?.lang ?? ""), {
      timeout: 180_000,
    })
    .toBe("de");
  // The German layout replaces the English one it split from.
  await expect
    .poll(
      () =>
        page.evaluate(() => {
          const s = (window as any).__plumbline;
          return s.paneVerseGeom[1]?.size ?? 0;
        }),
      { timeout: 60_000 },
    )
    .toBeGreaterThan(3);

  // A verse deep enough for the two texts' heights to have drifted apart.
  const verse = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    const a = s.paneVerseGeom[0] as Map<number, { y: number; h: number }>;
    const b = s.paneVerseGeom[1] as Map<number, { y: number; h: number }>;
    let best: number | null = null;
    for (const [v, g] of a) {
      const gb = b.get(v);
      if (gb && Math.abs(gb.y - g.y) > 40 && g.y > 200) best = best === null || v < best ? v : best;
    }
    return best;
  });
  // PRECONDITION, not an assumption: if the two layouts happened to agree
  // everywhere, alignment and offset-copy would be indistinguishable and this
  // test would prove nothing. Genesis 1 in Luther German does not agree.
  expect(verse, "the two layouts must actually differ somewhere").not.toBeNull();

  await chain(page).click();
  const targets = await page.evaluate((v) => {
    const s = (window as any).__plumbline;
    return { en: s.paneVerseGeom[0].get(v), de: s.paneVerseGeom[1].get(v) };
  }, verse!);
  await scrollPane(page, 0, targets.en!.y + 1);

  // The German pane lands at ITS OWN line for that verse — which the
  // precondition guarantees is measurably not the English offset.
  await expect
    .poll(() => paneTop(page, 1), { timeout: 10_000 })
    .toBeGreaterThan(targets.de!.y - 30);
  expect(await paneTop(page, 1)).toBeLessThan(targets.de!.y + targets.de!.h + 30);
  expect(Math.abs((await paneTop(page, 1)) - (targets.en!.y + 1))).toBeGreaterThan(30);
});
