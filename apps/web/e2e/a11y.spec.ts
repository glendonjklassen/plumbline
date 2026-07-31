import { expect, test, type Page } from "@playwright/test";

// The reader is a <canvas>, and a canvas holds no text. So to a screen reader,
// to the browser's own Ctrl+F, and to a translate feature, the Bible text was
// NOT THERE — the single most serious accessibility fact about this app, and the
// reason a reader could not find a phrase on the chapter in front of them with
// their own browser.
//
// ReaderPane now mirrors the display list into hidden-but-present DOM text, and
// the canon strip — an interactive canvas with no keyboard story at all — is a
// slider a keyboard can reach and drive.
//
// These assert on CHROMIUM'S OWN ACCESSIBILITY TREE wherever they can
// (Accessibility.getFullAXTree over CDP), not only on DOM text: the point is
// what assistive tech sees, and DOM text can be present while an `aria-hidden`
// or a `display: none` somewhere above it keeps the tree empty. The two are not
// the same assertion, which is exactly why hiding the mirror the WRONG way
// would pass a DOM-only test.
//
// Chromium-only by construction (CDP has no WebKit equivalent), which costs
// nothing: the WebKit project in playwright.config.ts is grepped down to the
// offline set, so nothing here is ever asked to run there.
//
// Mutation-tested 2026-07-29 (working rules: break the fix, watch it fail,
// restore) — the exact failures are recorded on each test.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const est = page.getByRole("button", { name: "Established believer" });
  await expect(est.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  // A fresh profile opens at John 3; wait for the layout, not just the canvas —
  // the mirror is built from the display list, so nothing is there before it.
  await expect(page.locator('.pane .mirror p[data-verse="16"]')).toHaveCount(1, { timeout: 90_000 });
}

interface AxNode {
  role: string;
  name: string;
  value: string;
  /** A slider's spoken value (`aria-valuetext`), which is what matters here: the
   *  numeric `aria-valuenow` is "book 43 of 66", and nobody can act on that. */
  valuetext: string;
  /** `"polite"` / `"assertive"` on a live region, `""` otherwise. */
  live: string;
}

/**
 * Chromium's accessibility tree, flattened to the nodes assistive tech is
 * actually offered.
 *
 * `ignored` nodes are dropped — that is where an `aria-hidden` subtree lands, so
 * keeping them would make "hidden from screen readers" indistinguishable from
 * "exposed". `InlineTextBox` nodes are dropped too: they are the line-box
 * children of a StaticText and repeat its content, which would make any count of
 * how many times a phrase is reported meaningless.
 */
async function axTree(page: Page): Promise<AxNode[]> {
  const cdp = await page.context().newCDPSession(page);
  const { nodes } = (await cdp.send("Accessibility.getFullAXTree" as any)) as any;
  await cdp.detach();
  return (nodes as any[])
    .filter((n) => !n.ignored && n.role?.value !== "InlineTextBox")
    .map((n) => ({
      role: String(n.role?.value ?? ""),
      name: String(n.name?.value ?? ""),
      value: String(n.value?.value ?? ""),
      valuetext: String(
        (n.properties ?? []).find((p: any) => p.name === "valuetext")?.value?.value ?? "",
      ),
      live: String((n.properties ?? []).find((p: any) => p.name === "live")?.value?.value ?? ""),
    }));
}

/**
 * Chromium's live regions and the text each one currently holds.
 *
 * A live region's own accessible NAME is empty — `role="status"` does not take
 * its name from its contents — so what a screen reader would speak is the text
 * of the subtree, which means walking `childIds` rather than reading one node.
 * `InlineTextBox` children are dropped for the same reason as above: they repeat
 * their StaticText parent and would say every book twice.
 */
async function axLiveRegions(page: Page): Promise<{ role: string; live: string; text: string }[]> {
  const cdp = await page.context().newCDPSession(page);
  const { nodes } = (await cdp.send("Accessibility.getFullAXTree" as any)) as any;
  await cdp.detach();
  const kept = (nodes as any[]).filter((n) => !n.ignored && n.role?.value !== "InlineTextBox");
  const byId = new Map(kept.map((n) => [n.nodeId, n]));
  const text = (n: any): string =>
    [
      String(n.name?.value ?? "") || String(n.value?.value ?? ""),
      ...(n.childIds ?? []).map((id: string) => byId.get(id)).filter(Boolean).map(text),
    ]
      .join(" ")
      .trim();
  return kept
    .filter((n) => (n.properties ?? []).some((p: any) => p.name === "live" && p.value?.value))
    .map((n) => ({
      role: String(n.role?.value ?? ""),
      live: String((n.properties ?? []).find((p: any) => p.name === "live").value.value),
      text: text(n),
    }));
}

/**
 * Wait out the background pipeline before driving anything the TOC feeds.
 *
 * The core / warm / analysis steps each end in a whole-cache `invalidate()`, and
 * that call drops the TOC as collateral: its keep-list tests a `"toc "` prefix
 * while the cache keys are `toc\0[]` (`session.svelte.ts`). For the round trip
 * before the refill lands the canon is EMPTY — the strip reports
 * `aria-valuetext=""`, and a click or a key on it does nothing at all. That is a
 * real bug of its own and not the one this file is about, so the two tests below
 * that depend on book NAMES wait for the pipeline to stop first, which is the
 * world a reader is in a minute after opening the app.
 *
 * A boot trace that has stopped growing says no further invalidation is coming
 * (same technique as `reading.spec.ts`). `expect.poll` rather than
 * `waitForFunction`: the working rules record a helper that "waited" for nothing
 * because an async predicate handed back a truthy promise on its first tick.
 */
async function settleBackground(page: Page): Promise<void> {
  let seen = -1;
  await expect
    .poll(
      async () => {
        const n = await page.evaluate(
          async () => (((await (window as any).__plumbline.rpc.bootTrace()) ?? []) as unknown[]).length,
        );
        const settled = n === seen && n > 10;
        seen = n;
        return settled;
      },
      { timeout: 120_000, intervals: [1500] },
    )
    .toBe(true);
}

/** The engine's own text for a verse — so these tests never transcribe scripture. */
async function verseBody(page: Page, refKey: string): Promise<string> {
  return await page.evaluate(
    async (r) => (await (window as any).__plumbline.engine.verse(r))?.body as string,
    refKey,
  );
}

// ── the text mirror ───────────────────────────────────────────────────────────

// Mutation: `aria-hidden="true"` on the mirror →
//   'Error: the accessibility tree has no John 3:16 — 32 nodes
//    expect(received).toBeGreaterThan(expected)  Expected: > 0  Received: 0'.
// Note WHICH assertion held under that mutation: `toHaveText` below passed
// perfectly happily. The DOM text and the accessibility tree are two different
// claims, and only the second one is the bug this file is about.
test("the chapter's words are in the accessibility tree, verse by verse", async ({ page }) => {
  await boot(page);
  const body = await verseBody(page, "John 3:16");
  expect(body).toContain("For God so loved the world");

  // The mirror says what the engine says — same words, same order, same
  // punctuation — and it carries the verse NUMBER, so it is navigable and
  // quotable rather than one undifferentiated blob.
  await expect(page.locator('.pane .mirror p[data-verse="16"]')).toHaveText(`16 ${body}`);

  // And it is what Chromium hands assistive tech.
  const tree = await axTree(page);
  const said = tree.filter((n) => n.name.includes(body) || n.value.includes(body));
  expect(said.length, `the accessibility tree has no John 3:16 — ${tree.length} nodes`).toBeGreaterThan(0);
  // ONCE. The canvas paints a picture of these same words, so if it were also
  // exposed the pane would report its chapter twice.
  expect(said.length, "John 3:16 is reported more than once").toBe(1);

  // The whole chapter, not a sample of it: one paragraph per verse.
  const verses = await page.evaluate(
    async () => (await (window as any).__plumbline.engine.chapterVerseCount("John", 3)) as number,
  );
  expect(verses).toBe(36);
  await expect(page.locator(".pane .mirror p")).toHaveCount(verses);
});

// The canvas is a rendering of the mirror, not a second copy of the chapter: it
// must not turn up in the tree as an object of its own.
//
// Mutation: dropping `aria-hidden="true"` from the reader canvas →
//   'Error: expect(received).toEqual(expected) // deep equality
//    - Array []  + Array [ "Canvas", ]'
test("the canvas itself does not report to a screen reader", async ({ page }) => {
  await boot(page);
  const tree = await axTree(page);
  expect(tree.filter((n) => n.role === "Canvas").map((n) => n.role)).toEqual([]);
});

// Ctrl+F is not driveable from Playwright (it is browser chrome), so this uses
// `window.find`, which runs the same "is this text findable in the rendered
// document" walk: it skips `display: none` and `visibility: hidden` subtrees
// exactly as find-in-page does. The hiding technique has to survive that.
//
// Mutation: `.mirror { display: none }` → 'Error: the browser cannot find the
//   chapter text  expect(received).toBe(expected)  Expected: true  Received:
//   false' — the find comes FIRST here for that reason: it is the outcome, and
//   the style checks below it are only the explanation.
test("the reader can find a phrase on the chapter with their own browser", async ({ page }) => {
  await boot(page);
  const phrase = "For God so loved the world";

  // Scrolled well into the chapter first: a find must not throw the reader back
  // to the top, which is what an in-flow mirror at scroll offset 0 would do.
  const scroller = page.locator(".pane .scroll");
  await scroller.evaluate((el) => (el.scrollTop = 600));
  const before = await scroller.evaluate((el) => el.scrollTop);
  expect(before).toBeGreaterThan(0);

  const found = await page.evaluate((q) => {
    getSelection()?.removeAllRanges();
    return (window as any).find(q, false, false, true) as boolean;
  }, phrase);
  expect(found, "the browser cannot find the chapter text").toBe(true);
  // And it selected the words, which is what makes a find quotable.
  expect(await page.evaluate(() => String(getSelection()))).toContain(phrase);
  expect(await scroller.evaluate((el) => el.scrollTop), "finding a phrase moved the reader").toBe(
    before,
  );

  // WHY it is findable: hidden the right way — still laid out, still exposed.
  const mirror = page.locator(".pane .mirror");
  const how = await mirror.evaluate((el) => {
    const cs = getComputedStyle(el);
    return { display: cs.display, visibility: cs.visibility, boxes: el.getClientRects().length };
  });
  expect(how.display).not.toBe("none");
  expect(how.visibility).not.toBe("hidden");
  expect(how.boxes).toBeGreaterThan(0);
  // Nothing in the mirror's ancestry may take it back out of the tree.
  expect(await mirror.evaluate((el) => !!el.closest("[aria-hidden='true']"))).toBe(false);
});

// Mutation: dropping `role="region"` and `aria-label` from the scroll wrapper →
//   "Error: expect(locator).toBeVisible() failed  Locator: getByRole('region',
//    { name: 'John 3' })  Error: element(s) not found".
test("the reading pane is a named region a screen reader can jump to", async ({ page }) => {
  await boot(page);
  await settleBackground(page); // the label is the book's NAME, so it needs the TOC
  const region = page.getByRole("region", { name: "John 3" });
  await expect(region).toBeVisible();
  // The text is INSIDE the named region — a label on an empty box is a gesture.
  await expect(region.locator(".mirror p")).toHaveCount(36);
  // Chromium agrees about the name, not just Playwright's own role engine.
  const tree = await axTree(page);
  expect(tree.filter((n) => n.role === "region" && n.name === "John 3")).toHaveLength(1);

  // The name follows the passage, and so does the text: a stale mirror would be
  // one chapter's words under another's name.
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Gen", 1));
  await expect(page.getByRole("region", { name: "Genesis 1" })).toBeVisible();
  const gen11 = await verseBody(page, "Gen 1:1");
  await expect(page.locator('.pane .mirror p[data-verse="1"]')).toHaveText(`1 ${gen11}`);
  await expect(page.getByRole("region", { name: "John 3" })).toHaveCount(0);
});

// The mirror is rebuilt from the display list ONCE PER LAYOUT. Scrolling must
// not touch it — it is not a second layout, and it may not land on the frame
// path of a pane that has to stay smooth on a phone.
//
// Mutation: adding `data-y={pane.scrollY}` to the mirror paragraphs — i.e.
//   putting the mirror on the scroll path at all →
//   'Error: the mirror was rebuilt while scrolling  expect(received)
//    .toBe(expected)  Expected: 0  Received: 216'
test("scrolling does not rebuild the mirror", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    const el = document.querySelector(".pane .mirror")!;
    (window as any).__mut = 0;
    new MutationObserver((recs) => ((window as any).__mut += recs.length)).observe(el, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
    });
  });
  const scroller = page.locator(".pane .scroll");
  for (const top of [120, 260, 400, 540, 680, 820]) {
    await scroller.evaluate((el, t) => (el.scrollTop = t), top);
    await page.waitForTimeout(60);
  }
  expect(await scroller.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  expect(
    await page.evaluate(() => (window as any).__mut as number),
    "the mirror was rebuilt while scrolling",
  ).toBe(0);
});

// ── the canon strip ───────────────────────────────────────────────────────────

// Mutations, both run: dropping `tabindex="0"` from the strip canvas → 'Error:
//   the canon strip is not reachable by keyboard  expect(received)
//   .toBe(expected)  Expected: true  Received: false' (40 tabs never land on
//   it). Dropping the `e.stopPropagation()` in its key handler → 'Error:
//   expect(locator).toHaveText(expected) failed  - Acts 1 ▾  + Acts 2 ▾' —
//   the shell's global ArrowRight had stepped the chapter as well.
test("the canon strip is reachable and operable by keyboard", async ({ page }) => {
  await boot(page);
  await settleBackground(page); // the strip is nothing without the canon
  const strip = page.getByRole("slider", { name: "Jump to a book" });
  await expect(strip).toHaveAttribute("aria-valuetext", "John");

  // Reached by TAB, from the top of the page — not by clicking it, which would
  // prove nothing about the keyboard.
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  let reached = false;
  for (let i = 0; i < 40 && !reached; i++) {
    await page.keyboard.press("Tab");
    reached = await page.evaluate(() => document.activeElement === document.querySelector(".strip canvas"));
  }
  expect(reached, "the canon strip is not reachable by keyboard").toBe(true);

  // Arrows step a book. Landing on Acts 1 (not Acts 2) is the assertion that
  // the strip KEEPS the key: the shell's global ArrowRight steps the chapter, so
  // a key that also bubbled would take the reader one chapter further.
  await page.keyboard.press("ArrowRight");
  await expect(strip).toHaveAttribute("aria-valuetext", "Acts");
  await expect(page.locator(".pane .nav .passage")).toHaveText("Acts 1 ▾");
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator(".pane .nav .passage")).toHaveText("John 1 ▾");

  // Home and End are the ends of the canon, not the ends of a chapter.
  await page.keyboard.press("Home");
  await expect(strip).toHaveAttribute("aria-valuetext", "Genesis");
  await page.keyboard.press("End");
  await expect(strip).toHaveAttribute("aria-valuetext", "Revelation");
  await expect(page.locator(".pane .nav .passage")).toHaveText("Revelation 1 ▾");

  // And it reaches the accessibility tree as ONE named slider, positioned where
  // the markup says it is.
  //
  // This used to assert the tree's `valuetext` was "Revelation" and it cannot:
  // Chromium stopped computing `aria-valuetext` for a canvas with
  // `role="slider"`. The node comes back with `valuetext: ""` and
  // `value: <aria-valuenow>` — verified against a tree dump on 2026-07-30, with
  // the DOM carrying `aria-valuetext="John"` at the same instant, and it fails
  // the same way on a tree with none of that day's changes in it. So what is
  // asserted here is what this shell controls (the attributes, checked three
  // times above) plus what the tree still reports, and the `valuetext: ""` below
  // is pinned deliberately: it is the browser behaviour this is working around,
  // and if Chromium ever starts computing it again, that line is what says so.
  //
  // The position a screen reader is actually GIVEN — "Revelation" and not "42" —
  // is a second channel, a polite live region beside the canvas, and it has a
  // test of its own directly below. Neither test stands without the other: the
  // attributes are right and unheard, the live region is heard and carries no
  // position of its own.
  const tree = await axTree(page);
  const sliders = tree.filter((n) => n.role === "slider" && n.name === "Jump to a book");
  expect(sliders, "the canon strip is not one named slider in the tree").toHaveLength(1);
  const now = await strip.getAttribute("aria-valuenow");
  expect(sliders[0].value, "the tree's position disagrees with aria-valuenow").toBe(String(now));
  expect(await strip.getAttribute("aria-valuetext")).toBe("Revelation");
  expect(
    sliders[0].valuetext,
    "Chromium is computing valuetext for a canvas slider again — the live region below may no longer be needed",
  ).toBe("");
});

// The second channel: the book, spoken.
//
// Because `aria-valuetext` is not computed for this node, everything the strip
// says about WHERE IT IS has to reach a screen reader some other way. A polite
// live region does it, and this test is about the region BEHAVING as one — in
// Chromium's own tree, with the book's name in it — rather than about markup
// that looks right. An `aria-live` attribute on an element the tree has dropped
// (`display: none`, `visibility: hidden`, an `aria-hidden` ancestor) reads
// perfectly in the DOM and announces nothing at all, which is precisely the
// class of mistake this file exists to catch.
//
// Mutations, all three run:
//   * deleting `spoken = book.name ?? book.id;` from `goTo` → 'Error: the canon
//     strip announced nothing when it moved  expect(received).toBe(expected)
//     Expected: "Acts"  Received: ""'.
//   * `.announce { display: none }` → 'Error: the announcement is not in the
//     accessibility tree, so nothing will speak it  expect(received).toEqual
//     (expected)  - Array ["polite"]  + Array []'. Every DOM assertion above it
//     still passes: the attribute is there, the text is there, and a screen
//     reader hears nothing. That gap is the reason the tree is consulted at all.
//   * removing BOTH `aria-live="polite"` and `role="status"` → 'Error:
//     expect(locator).toHaveAttribute() failed  Locator: locator(".strip
//     [aria-live]")  Error: element(s) not found'. Both have to go: Chromium
//     reports `live: "polite"` for either one on its own.
test("the canon strip announces the book it lands on", async ({ page }) => {
  await boot(page);
  await settleBackground(page); // it announces a NAME, so it needs the TOC

  const announce = page.locator(".strip [aria-live]");
  await expect(announce).toHaveAttribute("aria-live", "polite");
  // Silent until the strip is moved: a region that arrives already full would
  // speak the book on every page load, over whatever else is being read out.
  await expect(announce).toHaveText("");

  const strip = page.getByRole("slider", { name: "Jump to a book" });
  await strip.focus();
  await page.keyboard.press("ArrowRight");
  await expect(announce, "the canon strip announced nothing when it moved").toHaveText("Acts");

  // Chromium agrees it is a live region, and the book is inside it — the two
  // halves of "a screen reader will say this".
  const spoken = await axLiveRegions(page);
  const withBook = spoken.filter((r) => r.text.includes("Acts"));
  expect(
    withBook.map((r) => r.live),
    "the announcement is not in the accessibility tree, so nothing will speak it",
  ).toEqual(["polite"]);

  // It follows the strip, and it says the BOOK — not "42", and not a chapter.
  await page.keyboard.press("End");
  await expect(announce).toHaveText("Revelation");

  // And it stays quiet for everything that is not the strip moving. Stepping a
  // chapter is the same book, and navigating from elsewhere already announces
  // itself through the pane's own region — a second voice would talk over it.
  await page.evaluate(() => (window as any).__plumbline.stepChapter(0, 1));
  await expect(page.locator(".pane .nav .passage")).toHaveText("Revelation 2 ▾");
  await expect(announce, "a chapter step is not a move along the canon").toHaveText("Revelation");
  await page.evaluate(() => (window as any).__plumbline.navigate(0, "Gen", 1));
  await expect(page.getByRole("region", { name: "Genesis 1" })).toBeVisible();
  await expect(
    announce,
    "navigating from somewhere else made the strip speak over whatever took the reader there",
  ).toHaveText("Revelation");
});
