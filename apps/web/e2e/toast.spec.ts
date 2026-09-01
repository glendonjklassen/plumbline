import { expect, test, type Page } from "@playwright/test";

// The toast, audited (maintainer, 2026-08-25: "a bit spazzy as to when they
// appear/disappear — sometimes they flash so you can't even read them").
//
// Four behaviours, each of which was a bug in Session.showToast / Shell.svelte:
//   1. A toast raised while another is showing gets its WHOLE stay. showToast
//      armed a fresh 2.2 s timer without disarming the last, so the PREVIOUS
//      one's timer cleared the new message early — the flash.
//   2. The stay is sized to the message. A flat 2.2 s fitted "Copied" and lost
//      "Backed up 12 files as plumbline-backup-2026-08-25.zip".
//   3. On a phone the toast sits ABOVE the bottom bar, not on it.
//   4. It shows over Present (z 60) — "Link copied" is raised from inside it,
//      and at the shell's z 50 it landed behind the screen that raised it.
//
// Timings are real-clock and the margins are wide on purpose: in (1) the stale
// timer would fire at 2.5 s, the assertion looks at 2.8 s, and the replacement's
// own clock runs to 4.5 s. A contended worker slips tens of milliseconds, not
// hundreds.
//
// Mutations (each against a rebuilt bundle):
//   * drop the clearTimeout in showToast → (1) red: the toast is gone at 2.8 s.
//   * a flat 2200 ms stay → (2) red at 3.4 s; a flat 7000 → (2) red on "Copied".
//   * `--toastBottom: 22px` → (3) red: the toast's bottom edge is inside the bar.
//   * `.toast.brief { z-index: 50 }` → (4) red: elementFromPoint finds Present.

async function boot(page: Page, size = { width: 1100, height: 800 }): Promise<void> {
  await page.setViewportSize(size);
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

const toast = (page: Page) => page.locator(".toast.brief");
const show = (page: Page, msg: string) => page.evaluate((m) => (window as any).__plumbline.showToast(m), msg);

test("a toast raised inside another's stay is not cut short by the first one's clock", async ({ page }) => {
  await boot(page);
  await show(page, "Copied");
  await expect(toast(page)).toHaveText("Copied");
  await page.waitForTimeout(1500);
  const second = "Tagged Isaiah 53:5 — Atonement";
  await show(page, second);
  await expect(toast(page)).toHaveText(second);
  // 2.8 s after the first was shown: its timer, had it survived, has fired.
  // The second is 1.3 s into a stay of at least 2.5 s.
  await page.waitForTimeout(1300);
  await expect(toast(page), "the previous toast's timer took the new one down").toHaveText(second);
});

test("a long message stays longer than a short one, and both do go", async ({ page }) => {
  await boot(page);
  const long = "Backed up 12 files as plumbline-backup-2026-08-25.zip";
  await show(page, long);
  await expect(toast(page)).toHaveText(long);
  // 3.4 s in: past the old flat 2.2 s, and past the 2.5 s floor a short
  // message gets.
  await page.waitForTimeout(3400);
  await expect(toast(page), "a 52-character message was given a one-word stay").toHaveText(long);
  // …and it does leave: a toast that stayed would be a notice.
  await expect(toast(page)).toHaveCount(0, { timeout: 6000 });

  await show(page, "Copied");
  await expect(toast(page)).toHaveText("Copied");
  await expect(toast(page), "a one-word toast is not a seven-second one").toHaveCount(0, { timeout: 3500 });
});

test("on a phone the toast sits above the bottom bar, not on it", async ({ page }) => {
  await boot(page, { width: 360, height: 740 });
  await show(page, "Marked read — today");
  await expect(toast(page)).toHaveText("Marked read — today");
  const t = await toast(page).boundingBox();
  const nav = await page.locator(".bottom-nav").boundingBox();
  expect(t, "no toast box").toBeTruthy();
  expect(nav, "no bottom bar at phone width").toBeTruthy();
  expect(t!.y + t!.height, "the toast lies over the destinations").toBeLessThanOrEqual(nav!.y + 0.5);
});

test("the toast shows over Present", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.showPresent = true;
    s.showToast("Link copied");
  });
  await expect(page.locator(".present")).toBeVisible();
  await expect(toast(page)).toHaveText("Link copied");
  const box = (await toast(page).boundingBox())!;
  const onTop = await page.evaluate(
    ([x, y]) => {
      const el = document.elementFromPoint(x, y);
      return !!el && !!el.closest(".toast.brief");
    },
    [box.x + box.width / 2, box.y + box.height / 2],
  );
  expect(onTop, "Present (z 60) is covering the toast").toBe(true);
});
