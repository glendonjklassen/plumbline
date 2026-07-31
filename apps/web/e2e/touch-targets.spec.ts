import { expect, test, type Page } from "@playwright/test";

// Every control in the chrome is at least 44×44.
//
// A CLASS test, for the same reason surfaces.spec.ts is one: the floor kept
// being stated per component, so the controls that missed it were exactly the
// ones nobody thought to state it on — the search glass (20px wide), the ≡
// (27px), every context-menu row, the Present stepbar, the study sheet's ✕.
// There is one rule now (`app.css`), and this is the thing that notices when a
// new dialog arrives with a control that rule cannot reach.
//
// So it does not test the rule. It opens every surface a reader can raise at a
// phone width and MEASURES each button actually on screen, which is the only
// claim worth making: a `min-height` some scoped selector out-specifies is a
// declaration, not a target.
//
// 44 and not 48: Material's minimum is 48dp and the Compose shell meets it, but
// this shell is also a desktop app, and 44 is the number WCAG's target-size
// criterion and iOS both use. The controls that matter most on a phone — the
// destination bar, the passage grids — are 52 by their own rules and stay there.

const PHONE = { width: 390, height: 844 };
const FLOOR = 44;

async function boot(page: Page): Promise<void> {
  await page.setViewportSize(PHONE);
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/**
 * The controls the floor deliberately does not reach, and why.
 *
 * Kept as selectors with their reasons rather than as a fudge factor: an
 * exemption should have to name itself, so that reading this list is enough to
 * tell whether the floor has been quietly abandoned somewhere.
 */
const EXEMPT: { selector: string; why: string }[] = [
  {
    selector: ".blocks .link",
    why: "a cross-reference inside a sentence of study text: aimed at as a word, and inflating it would set the paragraph's line height to 44px — BlockList.svelte turns the floor off there on purpose, and the second test below holds it off",
  },
  {
    selector: ".pane .nav button",
    why: "the reader's own chapter arrows, sized deliberately at 44 tall × 40 wide by ReaderPane's scoped rules, which out-specify the global floor",
  },
];

interface Small {
  where: string;
  label: string;
  w: number;
  h: number;
}

/** Every on-screen, non-exempt button that is under the floor. */
async function tooSmall(page: Page, where: string): Promise<Small[]> {
  return await page.evaluate(
    ({ where, floor, exempt }) => {
      const out: { where: string; label: string; w: number; h: number }[] = [];
      for (const el of Array.from(document.querySelectorAll("button"))) {
        if (exempt.some((sel) => el.matches(sel))) continue;
        const r = el.getBoundingClientRect();
        // Nothing rendered — a closed surface still in the DOM, or a control the
        // media query hides. There is no target, so there is nothing to say.
        if (r.width === 0 && r.height === 0) continue;
        if (getComputedStyle(el).visibility === "hidden") continue;
        if (r.width >= floor - 0.5 && r.height >= floor - 0.5) continue;
        const label =
          el.getAttribute("aria-label") ||
          (el.textContent ?? "").trim().slice(0, 40) ||
          el.className ||
          "<unlabelled>";
        out.push({ where, label, w: Math.round(r.width), h: Math.round(r.height) });
      }
      return out;
    },
    { where, floor: FLOOR, exempt: EXEMPT.map((e) => e.selector) },
  );
}

/**
 * Every surface a reader can raise, and how to raise it — driven through session
 * state wherever it can be, so the table stays about the tap targets and not
 * about however each one happens to be reached (surfaces.spec.ts, same reason).
 *
 * `settle` proves the surface is really up before anything is measured: without
 * one, a surface that failed to open would contribute no buttons and read as a
 * pass. The two entries with `clicks` are the states that live in component
 * state rather than the session and cannot be set from outside — they come last,
 * because what they leave on screen cannot be cleared by `dismissTransient`.
 */
const SURFACES: { name: string; open?: string; clicks?: string[]; settle: string }[] = [
  { name: "the header and the reader", settle: "header .glass" },
  { name: "the verse context menu", open: `s.contextMenu = { x: 40, y: 120, refKey: "John 3:16" }`, settle: ".menu .ref" },
  { name: "the study sheet", open: `s.panel = { kind: "guide" }`, settle: '[data-surface="study panel"] .close' },
  { name: "the tag picker", open: `s.tagPickFor = "John 3:16"`, settle: '[data-surface="tag picker"]' },
  { name: "the thread picker", open: `s.threadPickFor = "John 3:16"`, settle: '[data-surface="thread picker"]' },
  { name: "the passage picker", open: `s.memorizePassageFrom = "John 3:16"`, settle: '[data-surface="passage picker"] .grid button' },
  { name: "the passage navigator", open: `s.bookNavFor = 0`, settle: ".dialog .grid.books button" },
  { name: "mark chapter read", open: `s.markReadFor = { book: "Gen", chapter: 1 }`, settle: '[data-surface="mark read"]' },
  { name: "history", open: `s.showHistory = true`, settle: '[data-surface="history"]' },
  { name: "settings", open: `s.showSettings = true`, settle: '[data-surface="settings"] .done' },
  { name: "keyboard shortcuts", open: `s.showShortcuts = true`, settle: ".dialog .close" },
  { name: "explore", open: `s.screen = "explore"`, settle: ".ex-card" },
  { name: "the memorize hub", open: `s.memorize = { view: "hub" }; s.screen = "memorize"`, settle: ".screen .actions button" },
  {
    name: "the memorize drill",
    // A card has to exist for the drill to have anything in it; `only` then
    // drives that one card without waiting on the due schedule.
    open:
      `await s.author("memoryAdd", "John 3:16", new Date().toISOString());` +
      `s.memorize = { view: "review", only: "John 3:16" }; s.screen = "memorize"`,
    settle: ".modes button",
  },
  { name: "Present, picking a thread", open: `s.showPresent = true`, settle: ".present .pick" },
  {
    name: "Present, a passage on screen",
    open: `s.showPresent = true`,
    clicks: [".present .pick", ".present .entry"],
    settle: ".present .stepbar button",
  },
  { name: "the ≡ menu", clicks: ['header [aria-label="Menu"]'], settle: ".menu-host .menu button" },
];

// Mutation: deleting `min-height`/`min-width` from the `button, summary` rule in
//   app.css → 'Error: these controls are under the 44px tap floor:
//     the header and the reader — "Open search" is 20×22
//     the header and the reader — "Menu" is 27×22
//     the verse context menu — "Copy" is 198×27
//     the study sheet — "Close panel" is 24×22
//     …'  — the sweep names every one it finds, so the failure IS the punch list.
test("every control in the chrome clears the 44px tap floor", async ({ page }) => {
  await boot(page);

  const small: Small[] = [];
  for (const s of SURFACES) {
    await page.evaluate(() => (window as any).__plumbline.dismissTransient());
    if (s.open) await page.evaluate(`(async () => { const s = window.__plumbline; ${s.open}; })()`);
    for (const sel of s.clicks ?? []) await page.locator(sel).first().click();
    await expect(page.locator(s.settle).first(), `${s.name} should open`).toBeVisible({ timeout: 20_000 });
    small.push(...(await tooSmall(page, s.name)));
  }

  // The sweep has to have SEEN something, or an empty result is a green test over
  // a page on which nothing ever opened.
  expect(
    small.length + (await page.evaluate(() => document.querySelectorAll("button").length)),
    "no buttons were measured at all",
  ).toBeGreaterThan(3);

  expect(
    small.map((s) => `${s.where} — "${s.label}" is ${s.w}×${s.h}`),
    "these controls are under the 44px tap floor",
  ).toEqual([]);
});

// The other half of the same rule, and the half a blanket floor gets wrong: a
// `<button>` is blockified to inline-block whatever `display: inline` says, so a
// global `min-height: 44px` DOES reach the cross-references inside study prose —
// and a 44px word sets its whole paragraph's line height to 44px. The study
// panel would become double-spaced text with the links floating in bands of
// white.
//
// Mutation: removing `min-height: 0; min-width: 0` from `.link` in
//   BlockList.svelte → 'Error: an in-sentence study link was inflated into a
//   44px tap target, so the paragraph around it is now 44px per line
//   expect(received).toBeLessThan(expected)  Expected: < 30  Received: 44'.
test("the floor does not inflate the links inside study prose", async ({ page }) => {
  await boot(page);
  // A word study, NOT "Guide & about": the guide's prose is a block list with no
  // linked runs at all (measured — 0 `.link` in it), so it cannot demonstrate
  // anything about links. A word study is core Strong's, needs no analysis pack,
  // and puts its links inside running paragraphs, which is the case that breaks.
  await page.evaluate(
    () => ((window as any).__plumbline.panel = { kind: "wordStudy", refKey: "John 3:16", tokenIndex: 0 }),
  );
  const link = page.locator('[data-surface="study panel"] .blocks .link').first();
  await expect(link).toBeVisible({ timeout: 20_000 });

  const box = (await link.boundingBox())!;
  expect(
    box.height,
    "an in-sentence study link was inflated into a 44px tap target, so the paragraph around it is now 44px per line",
  ).toBeLessThan(30);

  // And the paragraph it sits in still reads as prose rather than as a stack of
  // bands: total height over the number of line boxes.
  const line = await link.evaluate((el) => {
    const p = el.closest("p")!;
    return p.getBoundingClientRect().height / Math.max(1, p.getClientRects().length);
  });
  expect(line, "the study paragraph's line height").toBeLessThan(30);
});
