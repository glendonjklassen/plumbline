import { expect, test } from "@playwright/test";

// The status bar follows the READER'S theme, whatever the phone's own scheme.
// index.html ships two media-scoped theme-color tags (the pre-script paint —
// manifest.spec.ts pins those); once the palette is known the app rewrites
// them. It rewrote only the FIRST, the light-scoped one — so a dark-mode phone,
// whose UA reads the SECOND, kept Theme::Dark's paper under a light reader
// theme. Chrome then drew light status-bar icons over the cream the page paints
// beneath its transparent bar: the clock and battery washed out (maintainer,
// 2026-08-25). Every tag has to carry the resolved paper.

test("every theme-color tag carries the reader's paper, on a dark-mode device too", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });

  // A LIGHT reader theme on a DARK device — the mismatch that exposed it.
  const paper = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.theme = "light";
    s.applyTheme();
    return s.palette.paper as string;
  });
  expect(paper.toLowerCase()).toBe("#fcf9f4");
  const contents = await page.evaluate(() =>
    [...document.querySelectorAll('meta[name="theme-color"]')].map((m) => m.getAttribute("content")),
  );
  expect(contents.length, "the media-scoped pair is still there to be rewritten").toBe(2);
  for (const c of contents) expect(c?.toLowerCase(), "a tag the UA may be reading was left stale").toBe(paper.toLowerCase());
});
