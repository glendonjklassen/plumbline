import { expect, test, type Page } from "@playwright/test";

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
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

  // The menu keeps utilities ONLY — the whole point of moving them out.
  await page.getByLabel("Menu").click();
  const menu = page.locator(".menu");
  await expect(menu.getByRole("button", { name: "Settings" })).toBeVisible();
  for (const gone of ["Study", "Preach", "Share", "Sing"]) {
    await expect(menu.getByRole("button", { name: gone })).toHaveCount(0);
  }
});

// The phone header used to be TWO strips of chrome: the app's name, a search
// glass, a bordered "Share" and the ≡ on one row, and the pane's own
// ‹ John 3 ▾ › nav on a second one underneath. Android has never looked like
// that — it has one bar, no title, and icons — and the doubled strip cost ~40px
// of a phone screen to say what one row already said (feedback 2026-08-02).
//
// Mutation-tested 2026-08-02: restoring the `.title` span, or dropping the
// `.reading :global(.pane > .nav) { display: none }` rule, each makes the height
// assertion fail. The height is derived from the header's own computed
// min-height rather than a constant, so a font-size or padding change cannot
// silently turn this green.
test("phone: one bar of chrome above the text, with no app title", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);

  // The name of the app is not a control and does not earn a phone's width.
  await expect(page.locator("header .title")).toHaveCount(0);
  await expect(page.getByText("Plumbline", { exact: true })).toHaveCount(0);

  // The chapter nav is IN the header, and the pane's own strip is gone.
  const inHeader = page.locator("header .chapter-nav");
  await expect(inHeader).toBeVisible();
  await expect(inHeader.locator(".passage")).toHaveText(/\w+ \d+ ▾/);
  await expect(page.locator(".pane > .nav")).toBeHidden();

  // Everything above the text is ONE bar: the reader's first line of scripture
  // starts within a hair of the header's own height, not two strips down.
  const header = (await page.locator("header").boundingBox())!;
  const canvas = (await page.locator(".pane canvas").first().boundingBox())!;
  expect(
    canvas.y - (header.y + header.height),
    "there is a second strip of chrome between the header and the text",
  ).toBeLessThan(12);

  // Share is a DESTINATION now (the bar role), not a header icon — the QR
  // and link live on its screen.
  await page.locator(".bottom-nav").getByRole("button", { name: "Share" }).click();
  await expect(page.getByText("Share Plumbline")).toBeVisible();
});

test("phone: the church rides the Share destination and Welcome the ≡ menu", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await page.evaluate(() => localStorage.setItem("plumbline:intro", "new"));
  await boot(page);
  await page.evaluate(() =>
    (window as any).__plumbline.setChurch({ name: "Grace Chapel", info: "Sundays 10am", url: "" }),
  );

  // The ≡ holds UTILITIES: Welcome yes (for every reader), Church no — the
  // church lives on the Share screen, beside the QR its setting feeds.
  await page.getByLabel("Menu").click();
  await expect(page.locator(".menu").getByRole("button", { name: "Welcome" })).toBeVisible();
  await expect(page.locator(".menu").getByRole("button", { name: "Church" })).toHaveCount(0);
  await page.keyboard.press("Escape");

  await page.locator(".bottom-nav").getByRole("button", { name: "Share" }).click();
  await expect(page.getByText("with Grace Chapel")).toBeVisible();
  await expect(page.getByRole("button", { name: "Church" })).toBeVisible();

  // And the header is still one row high with everything set.
  await page.locator(".bottom-nav").getByRole("button", { name: "Read" }).click();
  const header = (await page.locator("header").boundingBox())!;
  expect(header.height).toBeLessThan(90);
});

// Present was the one surface that covered the whole chrome, so the four
// destinations vanished the moment you opened it — leaving a ✕ as the only way
// back, on the one screen someone is using in front of other people (feedback
// 2026-07-28). Both of its states are checked: the picker, and a passage actually
// up on screen. They are the same element, and it would be easy to fix the first
// and not notice the second.
test("phone: Present keeps the four destinations, picking and presenting alike", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);
  const nav = page.locator(".bottom-nav");

  await nav.getByRole("button", { name: "Preach" }).click();
  await expect(page.locator(".present")).toBeVisible();
  await expect(nav, "the picker covered the bottom bar").toBeVisible();
  await expect(nav.locator("button.on")).toHaveText(/Preach/);

  // ...and with something actually being presented. UNCONDITIONALLY: the stock
  // set seeds the Romans Road thread, so the picker always has one to choose, and
  // an `if` around this half would quietly skip the state that matters on any run
  // where the seeding broke.
  const picks = page.locator(".present .pick");
  await expect(picks.first(), "the stock set should give the picker a thread").toBeVisible();
  await picks.first().click();
  await expect(page.locator(".present.picking")).toHaveCount(0);
  await expect(nav, "presenting a passage covered the bottom bar").toBeVisible();
  await expect(nav.getByRole("button", { name: "Read" })).toBeVisible();

  // GEOMETRY, not just visibility. `toBeVisible` is satisfied by an element that
  // is painted over by something else — the first version of this test passed
  // with Present still spanning the full viewport, because raising the bar's
  // z-index kept it clickable and that was all anything checked. What the fix
  // actually has to guarantee is that Present STOPS ABOVE the bar, so its own
  // controls are not sitting underneath it.
  const pres = await page.locator(".present").boundingBox();
  const bar = await nav.boundingBox();
  expect(pres && bar, "both surfaces should be on screen").toBeTruthy();
  expect(
    Math.round(pres!.y + pres!.height),
    `Present runs to ${Math.round(pres!.y + pres!.height)}px but the bar starts at ` +
      `${Math.round(bar!.y)}px — it is underlapping the bar, so whatever Present ` +
      `draws at its bottom edge is hidden behind the destinations`,
  ).toBeLessThanOrEqual(Math.round(bar!.y));

  // The bar still works from in there — that is the point of keeping it.
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
  // NOT asserting that Memorize discards the Explore panel. Android's four
  // destinations are exclusive because it shows one screen at a time; the web
  // layers, and on a DESKTOP the study panel is a sidebar — keeping Explore
  // behind a fullscreen Present so it is still there on the way back is the right
  // behaviour there, and `go()` is shared by both widths. What matters, and is
  // asserted, is that the highlighted tab always names the surface actually in
  // front of the reader.

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
  // The bar sits BELOW the pane, not over it: a nav bar overlapping the last
  // line of scripture is worse than no nav bar.
  const pane = (await page.locator(".panes").boundingBox())!;
  const bar = (await page.locator(".bottom-nav").boundingBox())!;
  expect(bar.y).toBeGreaterThanOrEqual(pane.y + pane.height - 1);
  expect(bar.height).toBeGreaterThanOrEqual(48); // a real thumb target

  await page.setViewportSize({ width: 1280, height: 900 });
  await expect(page.locator(".bottom-nav")).toBeHidden();
  // …because at that width the destinations are first-class in the top bar.
  await expect(page.locator("nav.browse").getByRole("button", { name: "Study" })).toBeVisible();
});

// A DESTINATION REPLACES THE TOP BAR — it does not stack under the reader's.
//
// Explore, Memorize and the Hymnal each rendered below the READER'S bar, so a
// phone showed "‹ 1 Corinthians 7 ›" — with its search and share — above a
// second bar saying "Explore", advertising a passage that screen has nothing to
// do with. Present has always replaced the lot, and Android's destinations own
// the whole column (feedback 2026-08-02: "should probably just look like present
// does").
//
// Mutation-tested 2026-08-02: drop the
// `.frame:not([data-screen="read"]) > header { display: none }` rule and each
// destination goes red on the header still being visible.
test("phone: a destination shows its own bar and not the reader's", async ({ page }) => {
  await page.setViewportSize({ width: 430, height: 860 });
  await boot(page);

  // Read keeps the reader's bar, chapter nav and all — the baseline.
  await expect(page.locator("header")).toBeVisible();
  await expect(page.locator("header .chapter-nav")).toBeVisible();

  for (const label of ["Study", "Share", "Sing"]) {
    await page.locator(".bottom-nav").getByRole("button", { name: label }).click();

    // The reader's bar is gone: no chapter nav, no search, no share.
    await expect(page.locator("header"), `${label} still shows the reader's bar`).toBeHidden();

    // Exactly ONE bar of chrome, and it names this destination.
    const bar = page.locator(".screen .bar, section .bar").first();
    await expect(bar).toBeVisible();
    await expect(bar.locator("h2")).toHaveText(label);

    // It clears the status bar, which the header used to be carrying.
    const top = await bar.evaluate((el) => parseFloat(getComputedStyle(el).paddingTop));
    expect(top, `${label}'s bar lost the safe-area inset`).toBeGreaterThanOrEqual(10);

    // And back returns to the reader, whose bar comes back with it.
    await page.locator(".bottom-nav").getByRole("button", { name: "Read" }).click();
    await expect(page.locator("header")).toBeVisible();
  }
});
