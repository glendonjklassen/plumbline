import { expect, test } from "@playwright/test";

// Guide and About are two cards, each with its own row in the ≡ menu, and About
// opens with three verses read from the open Bible.
//
// Reasoned rather than mutated:
//   - the verses are matched on their own words, so a card that lists the three
//     references without their text fails, as does one built without an engine
//     (the static path has no Bible, and would show none of them).
//   - the guide is checked for the ABSENCE of the covenant heading, which is how
//     About inlined at the guide's end would show.
//   - the version line is looked for on About, where it now lives alone.
test("About is its own card and opens with its three verses", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

  await page.getByLabel("Menu").first().click();
  await page.locator(".menu").getByRole("button", { name: "About", exact: true }).click();
  const card = page.locator('[data-surface="study panel"]').first();
  await expect(card).toContainText("About Plumbline", { timeout: 30_000 });
  await expect(card).toContainText("All scripture is given by inspiration of God");
  await expect(card).toContainText("2 Timothy 3:16");
  await expect(card).toContainText("war a good warfare");
  await expect(card).toContainText("we wrestle not against flesh and blood");
  await expect(card).toContainText("THE COVENANT");
  await expect(card.locator(".version")).toBeVisible();
  await expect(card).not.toContainText("Using Plumbline");

  await page.evaluate(() => ((window as any).__plumbline.panel = { kind: "guide" }));
  await expect(card).toContainText("Using Plumbline", { timeout: 30_000 });
  await expect(card).not.toContainText("THE COVENANT");
  await expect(card).not.toContainText("Whatever you came here to do");
});
