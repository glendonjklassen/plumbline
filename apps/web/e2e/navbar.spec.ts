import { expect, test, type Page } from "@playwright/test";

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

test("phone: the five destinations are in the bottom bar, not the menu", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  const nav = page.locator(".bottom-nav");
  await expect(nav).toBeVisible();
  await expect(nav.locator("button")).toHaveCount(5);
  for (const label of ["Read", "Study", "Preach", "Share", "Sing"]) {
    await expect(nav.getByRole("button", { name: label })).toBeVisible();
  }
  // Read is current on arrival, and the icons really are icons.
  await expect(nav.locator("button.on")).toHaveText(/Read/);
  await expect(nav.locator("svg")).toHaveCount(5);

  // The menu keeps utilities only; the destinations live in the bar.
  await page.getByLabel("Menu").click();
  const menu = page.locator(".menu");
  await expect(menu.getByRole("button", { name: "Settings" })).toBeVisible();
  for (const gone of ["Study", "Preach", "Share", "Sing"]) {
    await expect(menu.getByRole("button", { name: gone })).toHaveCount(0);
  }
});

// A phone gets ONE strip of chrome above the text, not two. Dies if the `.title`
// span comes back, or if the `.reading :global(.pane > .nav) { display: none }` rule
// is dropped. The gap is measured against the header's own box rather than a
// constant, so a font-size or padding change cannot silently turn it green.
test("phone: one bar of chrome above the text, with no app title", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);

  await expect(page.locator("header .title")).toHaveCount(0);
  await expect(page.getByText("Plumbline", { exact: true })).toHaveCount(0);

  // The chapter nav is in the header, and the pane's own strip is gone.
  const inHeader = page.locator("header .chapter-nav");
  await expect(inHeader).toBeVisible();
  await expect(inHeader.locator(".passage")).toHaveText(/\w+ \d+ ▾/);
  await expect(page.locator(".pane > .nav")).toBeHidden();

  // The first line of scripture starts within a hair of the header's own height.
  const header = (await page.locator("header").boundingBox())!;
  const canvas = (await page.locator(".pane canvas").first().boundingBox())!;
  expect(
    canvas.y - (header.y + header.height),
    "there is a second strip of chrome between the header and the text",
  ).toBeLessThan(12);

  // Share is a destination, not a header icon: the QR and link live on its screen.
  await page.locator(".bottom-nav").getByRole("button", { name: "Share" }).click();
  await expect(page.getByText("Share Plumbline")).toBeVisible();
});

test("phone: the church rides the Share destination, not the ≡ menu", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await page.evaluate(() => localStorage.setItem("plumbline:intro", "new"));
  await boot(page);
  await page.evaluate(() =>
    (window as any).__plumbline.setChurch({ name: "Grace Chapel", service: 600, url: "" }),
  );

  // The ≡ holds utilities only. Church is not among them — it lives on the Share
  // screen, beside the QR its setting feeds — and neither is Welcome, which went
  // with the first-run personas.
  await page.getByLabel("Menu").click();
  await expect(page.locator(".menu").getByRole("button", { name: "Settings" })).toBeVisible();
  await expect(page.locator(".menu").getByRole("button", { name: "Church" })).toHaveCount(0);
  await expect(page.locator(".menu").getByRole("button", { name: "Welcome" })).toHaveCount(0);
  await page.keyboard.press("Escape");

  await page.locator(".bottom-nav").getByRole("button", { name: "Share" }).click();
  await expect(page.getByText("with Grace Chapel")).toBeVisible();
  await expect(page.getByRole("button", { name: "Church" })).toBeVisible();

  // The header is still one row high with the church set.
  await page.locator(".bottom-nav").getByRole("button", { name: "Read" }).click();
  const header = (await page.locator("header").boundingBox())!;
  expect(header.height).toBeLessThan(90);
});

// Present used to cover the whole chrome, leaving a ✕ as the only way back. Both of
// its states are checked — the picker and a passage on screen — because they are the
// same element and it is easy to fix the first without noticing the second.
test("phone: Present keeps the four destinations, picking and presenting alike", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  const nav = page.locator(".bottom-nav");

  // Present is the headline card of the Preach hub.
  await nav.getByRole("button", { name: "Preach" }).click();
  await page.locator(".ex-card", { hasText: "Present" }).first().click();
  await expect(page.locator(".present")).toBeVisible();
  await expect(nav, "the picker covered the bottom bar").toBeVisible();
  await expect(nav.locator("button.on")).toHaveText(/Preach/);

  // Unconditional: the stock set always seeds a thread for the picker, so an `if`
  // here would silently skip the presenting state on a run where seeding broke.
  const picks = page.locator(".present .pick");
  await expect(picks.first(), "the stock set should give the picker a thread").toBeVisible();
  await picks.first().click();
  await expect(page.locator(".present.picking")).toHaveCount(0);
  await expect(nav, "presenting a passage covered the bottom bar").toBeVisible();
  await expect(nav.getByRole("button", { name: "Read" })).toBeVisible();

  // Geometry, not just visibility: `toBeVisible` is satisfied by an element painted
  // over by something else, so a full-viewport Present with a raised-z-index bar
  // would pass. Present must stop above the bar, not underlap it.
  const pres = await page.locator(".present").boundingBox();
  const bar = await nav.boundingBox();
  expect(pres && bar, "both surfaces should be on screen").toBeTruthy();
  expect(
    Math.round(pres!.y + pres!.height),
    `Present runs to ${Math.round(pres!.y + pres!.height)}px but the bar starts at ` +
      `${Math.round(bar!.y)}px — it is underlapping the bar, so whatever Present ` +
      `draws at its bottom edge is hidden behind the destinations`,
  ).toBeLessThanOrEqual(Math.round(bar!.y));

  // The bar still works from inside Present.
  await nav.getByRole("button", { name: "Read" }).click();
  await expect(page.locator(".present")).toHaveCount(0);
});

test("phone: the bottom bar switches destinations and tracks which is current", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  const nav = page.locator(".bottom-nav");

  await nav.getByRole("button", { name: "Study" }).click();
  await expect(nav.locator("button.on")).toHaveText(/Study/);

  // Memorize is a Study-hub card, and its screen lights the Study tab.
  await page.locator(".ex-card", { hasText: /^Memorize/ }).click();
  await expect(nav.locator("button.on")).toHaveText(/Study/);
  // Deliberately not asserted: that Memorize discards the Explore panel. The web
  // layers its surfaces, and on a desktop width the study panel is a sidebar, so
  // Explore staying behind a fullscreen surface is right; `go()` is shared by both
  // widths. What is asserted is that the lit tab names the surface in front.

  // Read is the absence of a destination: it clears whatever is layered over
  // the reader rather than opening anything.
  await nav.getByRole("button", { name: "Read" }).click();
  await expect(nav.locator("button.on")).toHaveText(/Read/);
  const state = await page.evaluate(() => {
    const s = (window as any).__plumbline;
    return { panel: s.panel, memorize: s.memorize, present: s.showPresent };
  });
  expect(state).toEqual({ panel: null, memorize: null, present: false });
  await expect(page.locator(".pane canvas").first()).toBeVisible();
});

test("the bottom bar does not cover the reader, and is absent on a desktop width", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  // The bar sits below the pane, never over the last line of scripture.
  const pane = (await page.locator(".panes").boundingBox())!;
  const bar = (await page.locator(".bottom-nav").boundingBox())!;
  expect(bar.y).toBeGreaterThanOrEqual(pane.y + pane.height - 1);
  expect(bar.height).toBeGreaterThanOrEqual(48); // a real thumb target

  await page.setViewportSize({ width: 1280, height: 900 });
  await expect(page.locator(".bottom-nav")).toBeHidden();
  // At that width the destinations are in the top bar instead.
  await expect(page.locator("nav.browse").getByRole("button", { name: "Study" })).toBeVisible();
});

// A destination replaces the top bar rather than stacking under the reader's: two
// bars would advertise a passage the destination has nothing to do with. Dies if the
// `.frame:not([data-screen="read"]) > header { display: none }` rule is dropped.
test("phone: a destination shows its own bar and not the reader's", async ({ page }) => {
  await page.setViewportSize({ width: 430, height: 860 });
  await boot(page);

  // The baseline: Read keeps the reader's bar, chapter nav and all.
  await expect(page.locator("header")).toBeVisible();
  await expect(page.locator("header .chapter-nav")).toBeVisible();

  // Sing's screen is the hymnal, whose bar carries the book's own name.
  for (const [label, barTitle] of [["Study", "Study"], ["Share", "Share"], ["Sing", "Hymnal"]] as const) {
    await page.locator(".bottom-nav").getByRole("button", { name: label }).click();

    await expect(page.locator("header"), `${label} still shows the reader's bar`).toBeHidden();

    // Exactly one bar of chrome, and it names this destination.
    const bar = page.locator(".screen .bar, section .bar").first();
    await expect(bar).toBeVisible();
    await expect(bar.locator("h2")).toHaveText(barTitle);

    // The destination's own bar now carries the status-bar safe-area inset.
    const top = await bar.evaluate((el) => parseFloat(getComputedStyle(el).paddingTop));
    expect(top, `${label}'s bar lost the safe-area inset`).toBeGreaterThanOrEqual(10);

    await page.locator(".bottom-nav").getByRole("button", { name: "Read" }).click();
    await expect(page.locator("header")).toBeVisible();
  }
});
