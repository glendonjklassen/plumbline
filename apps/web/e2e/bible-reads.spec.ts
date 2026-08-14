import { expect, test, type Page } from "@playwright/test";

// The lifetime counter: how many times this reader has been through the whole
// Bible. Seeded ONCE by hand — somebody arriving with thirty years behind them
// should not start at nought — and EARNED after that: nothing edits it, and the
// only thing that moves it is finishing the canon (maintainer, 2026-08-13).
//
// -1 is "never said", which is deliberately not 0: a reader who answers "none"
// has told us something and must not be asked again.

async function boot(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1100, height: 900 });
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

const openStudy = (page: Page) => page.getByRole("button", { name: "Study" }).first().click();
const reads = (page: Page) => page.evaluate(() => (window as any).__plumbline.config.bibleReads);

test("unset, it asks; answered, it states — and the number sticks", async ({ page }) => {
  await boot(page);
  await openStudy(page);

  // The invitation, not a number.
  const invite = page.locator(".reads.unset");
  await expect(invite).toBeVisible({ timeout: 15_000 });
  await invite.click();

  // A numeric field, which is what raises a phone's numpad.
  const field = page.getByRole("dialog", { name: /How many times/ }).locator("input");
  await expect(field).toHaveAttribute("inputmode", "numeric");
  await field.fill("7");
  await page.getByRole("button", { name: "OK" }).click();

  await expect(page.locator(".reads-n")).toHaveText("7", { timeout: 15_000 });
  expect(await reads(page)).toBe(7);
  // No longer a control: the invitation is gone and there is nothing to retype.
  await expect(page.locator(".reads.unset")).toHaveCount(0);
});

test("nought is an answer, not a refusal to answer", async ({ page }) => {
  await boot(page);
  await openStudy(page);
  await page.locator(".reads.unset").click();
  await page.getByRole("dialog", { name: /How many times/ }).locator("input").fill("0");
  await page.getByRole("button", { name: "OK" }).click();

  // 0 is a real answer and must stick — if "unset" were 0 this would ask again
  // every time, which is the reason the unset value is -1.
  await expect(page.locator(".reads-n")).toHaveText("0", { timeout: 15_000 });
  expect(await reads(page)).toBe(0);
  await expect(page.locator(".reads.unset")).toHaveCount(0);
});

test("cancelling leaves it unasked", async ({ page }) => {
  await boot(page);
  await openStudy(page);
  await page.locator(".reads.unset").click();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.locator(".reads.unset")).toBeVisible();
  expect(await reads(page)).toBe(-1);
});

test("finishing the canon credits exactly one read, however often you look", async ({ page }) => {
  await boot(page);
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    s.config.bibleReads = 2;
    s.config.bibleReadsCredited = false;
    s.saveConfig();
  });

  // Read the whole canon. `markRead` is the same call the navigator's
  // press-and-hold makes.
  await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    const books = (s.q("toc")?.books ?? []) as any[];
    for (const b of books) {
      for (let c = 1; c <= b.chapters; c++) await s.engine.readingMarkRead(b.id, c, "2026-08-10T12:00:00Z");
    }
    s.invalidate();
  }, undefined);

  await openStudy(page);
  await expect(page.locator(".reads-n")).toHaveText("3", { timeout: 60_000 });

  // Look again — and again. The credit is for FINISHING, not for visiting.
  await page.getByRole("button", { name: "Read" }).first().click();
  await openStudy(page);
  await expect(page.locator(".reads-n")).toHaveText("3", { timeout: 30_000 });
  expect(await reads(page)).toBe(3);
});
