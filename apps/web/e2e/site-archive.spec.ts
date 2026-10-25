import { expect, test } from "@playwright/test";

// The site's own archive — `plumbline-site.tar.gz`, which a release puts beside the app — is
// offered from the Backup dialog. It used to be a line under About, gated on the bare `about`
// panel, which nothing in the ≡ menu opens ("Guide & About" is the guide panel), so it shipped
// unreachable. This fails against that: the row is looked for on the Backup surface, by its
// copy, and its href must be the archive the release named.
//
// A local build has no archive.json (the release writes it), so the route supplies one. The
// service worker is blocked for that test: once sw.js controls the page it answers the fetch
// itself and `page.route()` never sees it (the trap CLAUDE.md names), and what it serves for an
// unknown path is the shell's index, which the dialog's shape check rejects — the row would be
// missing for the wrong reason. The second test wants exactly that behaviour, SW and all.

test.describe("with a release's archive", () => {
  test.use({ serviceWorkers: "block" });

  test("the backup dialog offers the whole site as one download", async ({
    page,
  }) => {
    await page.route("**/archive.json", (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          path: "plumbline-site.tar.gz",
          bytes: 35_050_755,
        }),
      }),
    );
    await page.goto("/");
    await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, {
      timeout: 90_000,
    });

    await page.evaluate(() => ((window as any).__plumbline.showBackup = true));
    const surface = page.locator('[data-surface="backup"]');
    await expect(surface).toBeVisible();

    const row = surface.getByRole("link", {
      name: "Download the site to keep or host yourself (35 MB)",
    });
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute("href", "plumbline-site.tar.gz");
    await expect(row).toHaveAttribute("download", "");
  });
});

test("without a release's archive the row is not shown", async ({ page }) => {
  // The preview answers an unknown path with the shell's index (not JSON), the shape check in the
  // dialog rejects it, and no half-made row with "NaN MB" appears.
  await page.goto("/");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, {
    timeout: 90_000,
  });
  await page.evaluate(() => ((window as any).__plumbline.showBackup = true));
  const surface = page.locator('[data-surface="backup"]');
  await expect(surface).toBeVisible();
  await expect(
    surface.getByRole("button", { name: "Show passphrase" }),
  ).toBeVisible();
  await expect(surface.getByRole("link")).toHaveCount(0);
});
