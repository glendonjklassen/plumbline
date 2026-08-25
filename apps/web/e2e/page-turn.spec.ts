import { expect, test, type Page } from "@playwright/test";

// Page-turn mode (maintainer ask, 2026-08-24): with `config.pageTurn` on, the
// reader keeps a tap gutter either side of the text — at least the 44px touch
// floor, whatever the margin slider says — and a tap there scrolls most of a
// screen: the right side ahead, the left side back. The point is a page-turner
// remote (a clicker that presses a fixed spot near an edge) driving the page
// hands-free.

async function boot(page: Page): Promise<void> {
  // Narrow: one pane, so the two gutters belong to the one column under test.
  await page.setViewportSize({ width: 640, height: 800 });
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

const scrollY = (page: Page): Promise<number> =>
  page.evaluate(() => (window as any).__plumbline.panes[0].scrollY);

test("gutter taps page the text — right ahead, left back, and only in the mode", async ({ page }) => {
  await boot(page);
  const canvas = page.locator(".pane canvas").first();
  const box = (await canvas.boundingBox())!;

  // MODE OFF: a gutter click is not a page turn. (It is not anything — the
  // margin holds no words to study.)
  await page.mouse.click(box.x + box.width - 8, box.y + box.height / 2);
  expect(await scrollY(page)).toBe(0);

  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.pageTurn = true;
    s.saveConfig();
  });

  // Right gutter → a page forward.
  await page.mouse.click(box.x + box.width - 8, box.y + box.height / 2);
  await expect.poll(() => scrollY(page)).toBeGreaterThan(0);
  const forward = await scrollY(page);

  // Left gutter → a page back, by the same portion.
  await page.mouse.click(box.x + 8, box.y + box.height / 2);
  await expect.poll(() => scrollY(page)).toBeLessThan(forward);

  // Already at the top, a back-tap stays clamped at zero rather than going
  // negative and leaving the pane above its own first line.
  await page.mouse.click(box.x + 8, box.y + box.height / 2);
  await page.mouse.click(box.x + 8, box.y + box.height / 2);
  expect(await scrollY(page)).toBe(0);
});

test("the mode guarantees the gutters exist even at the narrowest margin", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    // The slider's minimum is 16 — narrower than a fingertip. The mode must
    // widen the gutter to the 44px floor or the remote has nothing to press.
    s.config.sideMargin = 16;
    s.config.pageTurn = true;
    s.saveConfig();
  });
  const canvas = page.locator(".pane canvas").first();
  const box = (await canvas.boundingBox())!;
  // A tap 30px in — inside the guaranteed 44px gutter, but outside a 16px one —
  // still pages.
  await page.mouse.click(box.x + box.width - 30, box.y + box.height / 2);
  await expect.poll(() => scrollY(page)).toBeGreaterThan(0);
});
