import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

// The reader's language, end to end.
//
// Everything else about i18n is checked where it lives — the catalogue's
// completeness in `crates/core/src/i18n.rs`, the absence of stray literals in
// `scripts/check-i18n.mjs`, the splash's pre-engine copy in splash.spec.ts.
// What none of those can see is the join: that a device reporting German gets a
// German app, that a reader's own choice outranks the device, and that the
// choice survives being closed and reopened.
//
// It drives the REAL PICKER rather than writing the config directly, and that
// is not ceremony. The first draft of this file poked `config.language` and
// reloaded, and it passed against a `setLanguage` that reloaded the page in the
// same tick as the save — the worker had not yet written the config through to
// IndexedDB, so a reader who picked German watched the app reload and come back
// in English. Only the path a reader actually takes could have caught it.
//
// The strings asserted here are read from the catalogue rather than typed, so
// this file does not become a second place the German copy lives. What it
// hard-codes is the KEY, which is the contract.

const EN: Record<string, string> = JSON.parse(
  readFileSync(new URL("../../../crates/core/src/i18n/en.json", import.meta.url), "utf8"),
);
const DE: Record<string, string> = JSON.parse(
  readFileSync(new URL("../../../crates/core/src/i18n/de.json", import.meta.url), "utf8"),
);

/** Boot far enough that the reader is up and the chrome is painted, answering
 *  the first-run chooser by its CATALOGUE key — the whole point of this file is
 *  that the words on those buttons change with the locale. */
async function reader(page: Page, lang: Record<string, string>): Promise<void> {
  await page.goto("/");
  const est = page.getByRole("button", { name: lang["intro.pathEstablished"] });
  const canvas = page.locator(".pane canvas").first();
  await expect(est.or(canvas)).toBeVisible({ timeout: 90_000 });
  if (await est.isVisible().catch(() => false)) {
    await est.click();
    await page.getByRole("button", { name: lang["intro.start"] }).click();
  }
  await expect(canvas).toBeVisible({ timeout: 90_000 });
}

/** The bottom bar is the chrome that is always up, in every layout. */
const destinations = (page: Page) => page.locator(".bottom-nav");

/**
 * Pick a language in Settings, the way a reader does.
 *
 * `now` is the catalogue the app is CURRENTLY in — the menu and the Settings
 * item are labelled in it. `want` is the option's own label, and options are
 * endonyms, which is what makes them the one stable thing on this screen: a
 * reader hunting for German looks for "Deutsch" whatever the app is speaking.
 */
async function pick(page: Page, now: Record<string, string>, want: string): Promise<void> {
  await page.getByLabel(now["common.menu"]).click();
  await page.locator(".menu").getByRole("button", { name: now["shell.settings"] }).click();
  const dialog = page.locator('[data-surface="settings"]');
  await expect(dialog).toBeVisible();
  // STAMP THIS DOCUMENT FIRST. Everything else that looks like a "the switch
  // finished" signal is already true before the reload: the canvas is on screen,
  // and `setLanguage` writes `config.language` on the live session before it
  // reloads. Waiting on either returned instantly and every assertion after it
  // read the PRE-SWITCH page — which reported `lang=- de=false` and sent me
  // hunting a fix that was already correct (2026-08-03).
  //
  // A property on `window` cannot survive a new document, so its disappearance is
  // the one unambiguous "this is the page after the reload".
  await page.evaluate(() => ((globalThis as any).__beforeSwitch = true));
  await dialog.getByRole("radio", { name: want, exact: true }).check();
  await page.waitForFunction(
    () => !(globalThis as any).__beforeSwitch && !!(globalThis as any).__plumbline,
    undefined,
    { timeout: 120_000 },
  );
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 120_000 });
}

test.describe("a German device", () => {
  test.use({ locale: "de-DE" });

  // MUTATION: engine.worker.ts — pass `""` instead of `m.locale` to
  // `i18nCatalog`. Red here, and green everywhere else in the suite, because
  // this is the only test that boots with a locale at all.
  test("opens in German with nobody having chosen anything", async ({ page }) => {
    await reader(page, DE);

    await expect(destinations(page)).toContainText(DE["nav.read"]);
    await expect(destinations(page)).toContainText(DE["nav.sing"]);
    // Not a coincidence of similar words: these differ from the English — and
    // the English probe must not be a SUBSTRING of any German label either
    // ("Singen" contains "Sing", so nav.sing cannot carry the negative half).
    expect(DE["nav.read"]).not.toBe(EN["nav.read"]);
    for (const v of Object.entries(DE).filter(([k]) => k.startsWith("nav.")).map(([, v]) => v))
      expect(v).not.toContain(EN["nav.read"]);
    await expect(destinations(page)).not.toContainText(EN["nav.read"]);
  });

  // MUTATION: `i18n::resolve` — drop the `chosen` arm so it only ever reads the
  // device. Red here; the test above stays green, which is why there are two.
  test("a reader who picks English keeps it, device notwithstanding", async ({ page }) => {
    await reader(page, DE);
    await pick(page, DE, "English");

    await expect(destinations(page)).toContainText(EN["nav.sing"]);
    await expect(destinations(page)).not.toContainText(DE["nav.sing"]);
  });
});

test.describe("an English device", () => {
  test.use({ locale: "en-US" });

  // The mirror, and the one that catches a save that never lands: the setting
  // has to survive the reload the picker itself performs, and then a relaunch.
  test("a reader who picks German gets German, and it survives a relaunch", async ({ page }) => {
    await reader(page, EN);
    await expect(destinations(page)).toContainText(EN["nav.sing"]);

    await pick(page, EN, "Deutsch");
    await expect(destinations(page)).toContainText(DE["nav.sing"]);

    // BOOK NAMES are the other half, and the half that would have been missed:
    // they are not in the catalogue at all (canon.rs owns the English), so a
    // shell taking its strings from the catalogue and its book names from the
    // engine could have ended up half-translated.
    // Through the passage navigator, which lists every book as text. The canon
    // strip next to it is painted, not written, so it has nothing to assert on.
    await page.getByTitle(DE["pane.goTo"]).first().click();
    const nav = page.getByRole("dialog", { name: DE["booknav.title"] });
    // The navigator opens on the New Testament, so assert against a book that
    // is there — and one whose German is not a substring of its English, which
    // "Johannes" against "John" would have been.
    await expect(nav).toContainText("Offenbarung");
    await expect(nav).not.toContainText("Revelation");
    await nav.getByLabel(DE["common.close"]).click();

    // THE TEXT ITSELF, which is the other half of picking a language and the
    // half that needed a 2.4 MB download to get here. Asked of the engine rather
    // than read off the screen: the reader is a canvas.
    const verse = await page.evaluate(async () => {
      const s = (window as any).__plumbline;
      return (await s.rpc.call("verse", "John 3:16"))?.body ?? "";
    });
    expect(verse, `John 3:16 is not German: ${verse}`).toContain("Gott");
    expect(verse, "John 3:16 came back in English").not.toContain("God so loved");

    // Again with the document thrown away: the setting lives in the config, not
    // in this page — and neither does the corpus, which is in the depot.
    await page.goto("about:blank");
    await reader(page, DE);
    await expect(destinations(page)).toContainText(DE["nav.sing"]);
    const again = await page.evaluate(async () => {
      const s = (window as any).__plumbline;
      return (await s.rpc.call("verse", "John 3:16"))?.body ?? "";
    });
    expect(again, "the German text did not survive a relaunch").toContain("Gott");

    // WORD STUDY WORKS ON THE GERMAN TEXT: the corpus ships its own Strong's
    // tags (merge-strongs.py), and they must survive the idxcache the web
    // actually reads — the Rust tests read the JSONL, so a web-cache builder
    // that dropped the tags would fail nowhere but here. A token of John 3:16
    // carries a code, and its study card has a concordance link.
    const study = await page.evaluate(async () => {
      const s = (window as any).__plumbline;
      for (let i = 0; i < 8; i++) {
        const tok = await s.rpc.call("token", "John 3:16", i);
        if (tok?.strongs?.length) {
          const blocks = await s.rpc.call("wordStudyBlocks", "John 3:16", i, s.gates);
          return { code: tok.strongs[0], json: JSON.stringify(blocks) };
        }
      }
      return null;
    });
    expect(study, "no tagged token in German John 3:16 — the idxcache lost the tags").not.toBeNull();
    expect(study!.json, "the German study card has no concordance link").toContain("occ:");
  });
});

/** A boot-trace entry's value, or null. */
async function traced(page: Page, prefix: string): Promise<number | null> {
  const trace = (await page.evaluate(() => (window as any).__plumbline.rpc.bootTrace())) as [string, number][];
  return trace.find(([k]) => k.startsWith(prefix))?.[1] ?? null;
}

test.describe("a German reader's boot", () => {
  test.use({ locale: "en-US" });

  /**
   * ONE CORPUS IS INFLATED, NOT TWO.
   *
   * Two corpus caches ship now, and a German reader has both on the device. Stage
   * 1 used to gunzip and copy BOTH into the home on every launch — ~63 MB of work
   * and memory before any text appeared, against ~35 MB for an English reader,
   * and then the home evicted both. Only one is ever opened. It was the whole
   * answer to "German seems crazy slow to load" (UAT, 2026-08-03).
   *
   * Asserted through the home's own eviction figure, which is the number that
   * exposed it: `home evict after open (KB)` is what stage 1 put in the home and
   * the engine no longer needs. Two corpora is ~63,000 KB; one is ~28,000–35,000.
   *
   * MUTATION: in boot.ts, drop `skipOther` from both stage-1 calls. Red — the
   * evicted figure roughly doubles.
   */
  test("inflates the corpus it reads and not the other one", async ({ page }) => {
    await reader(page, EN);
    const english = await traced(page, "home evict after open");
    expect(english, "no eviction figure in the boot trace").not.toBeNull();

    await pick(page, EN, "Deutsch");
    const german = await traced(page, "home evict after open");
    expect(german, "no eviction figure after the switch").not.toBeNull();

    // The German corpus is SMALLER than the KJV's (no Strong's arrays), so a
    // German boot should evict LESS than an English one — and certainly not the
    // sum of the two, which is what loading both looks like.
    expect(
      german!,
      `a German boot put ${german} KB in the home; English put ${english} KB. ` +
        `Anything near their sum means both corpora were inflated.`,
    ).toBeLessThan(english! * 1.4);

    // Which corpus stage 1 actually chose — the trace says so out loud, so a
    // failure here names the cause instead of only the symptom.
    const trace = (await page.evaluate(() => (window as any).__plumbline.rpc.bootTrace())) as [string, number][];
    const chose = trace.find(([k]) => k.startsWith("corpus loaded"))?.[0];
    expect(chose, `stage 1 chose ${chose}; trace: ${JSON.stringify(trace)}`).toBe("corpus loaded (germanCorpus)");

    // AND THE TRACE MUST NOT LIE ABOUT IT. `hadIdxcache` only looked for the
    // KJV's cache, so a German boot that took the fast path was reported as
    // "cold corpus parse" — the first thing anyone reads when a launch is slow on
    // a device, pointing at a 19 MB re-parse that never happened (2026-08-03).
    // app.spec.ts asserts the English half of this same label.
    //
    // MUTATION: in home.ts, drop `|| pack.has(GERMAN_CACHE)`. Red here, green
    // there, which is why both exist.
    const open = trace.find(([k]) => k.startsWith("engine open"))?.[0];
    expect(open, `trace: ${JSON.stringify(trace)}`).toBe("engine open (idxcache fast path)");

    // And the text really is German, so this did not pass by loading neither.
    const verse = await page.evaluate(async () => {
      const s = (window as any).__plumbline;
      return (await s.rpc.call("verse", "John 3:16"))?.body ?? "";
    });
    expect(verse).toContain("Gott");
  });
});

test.describe("the guide", () => {
  test.use({ locale: "en-US" });

  /**
   * GUIDE & ABOUT IS THE LONGEST PROSE IN THE APP and it was the last English
   * left: about forty paragraphs of literals in `panel.rs`, so a German reader met
   * a German app right up to this card and then a wall of English (2026-08-04).
   *
   * The core proves its own half — `the_guide_is_readable_by_a_german_reader` in
   * `crates/core/src/panel/tests.rs` renders the card in German and refuses the
   * old English phrases. What only the shell can prove is the JOIN: that this
   * button asks the engine for the guide AFTER the language is set, rather than
   * handing back something built or cached in English. That is not a hypothetical
   * — the hymnal asked for "en" outright until UAT caught it two days ago.
   *
   * MUTATION: in `panel.rs`, make `guide_blocks()` call `guide_blocks_in(Lang::En)`
   * instead of `i18n::active()`. Red here.
   */
  test("opens in the reader's language", async ({ page }) => {
    await reader(page, EN);
    await pick(page, EN, "Deutsch");

    await page.getByLabel(DE["common.menu"]).click();
    await page.locator(".menu").getByRole("button", { name: DE["shell.guideAndAbout"] }).click();

    const card = page.locator('[data-surface="study panel"]').first();
    await expect(card).toContainText(DE["guide.title"], { timeout: 30_000 });
    // The German headings, and none of the English the card used to be built from.
    await expect(card).toContainText(DE["guide.memorize.title"]);
    await expect(card).toContainText(DE["about.covenant.title"]);
    await expect(card).not.toContainText(EN["guide.title"]);
    await expect(card).not.toContainText("HIDE IT IN YOUR HEART");
    await expect(card).not.toContainText("MAKE IT YOURS");
  });
});

/**
 * Lay one chapter out COLD, then ask for the very same layout again — the
 * worker's turn cache answers the second one — and return both costs in ms.
 *
 * The cached call is the calibration: it ships the identical display list over
 * the identical postMessage on this machine, and does none of the work. So
 * `cold / cached` is "how expensive is producing this list, in units of handing
 * it over", which is a budget the device sets for itself.
 */
async function layoutCostAndFloor(page: Page, book: string, chapter: number, width: number): Promise<[number, number]> {
  return page.evaluate(
    async ([book, chapter, width]) => {
      const s = (window as any).__plumbline;
      const cfg = { font: 20, width: width as number, lineSpacing: 1.45, versePerLine: false };
      const t0 = performance.now();
      await s.rpc.layout(book, chapter as number, cfg);
      const t1 = performance.now();
      // Same key, so this is served from the turn cache: transport only.
      await s.rpc.layout(book, chapter as number, cfg);
      const t2 = performance.now();
      return [t1 - t0, Math.max(t2 - t1, 0.2)] as [number, number];
    },
    [book, chapter, width] as [string, number, number],
  );
}

test.describe("a German reader's chapter turn", () => {
  test.use({ locale: "en-US" });

  /**
   * NOTHING IS REDONE PER WORD when a chapter is laid out.
   *
   * `i18n::catalog()` used to re-parse its JSON on every call, `t()` called it for
   * every lookup, and the wire layer turns EVERY WORD of a laid-out chapter into a
   * reference through `VRef::display()`. So one German Psalm 119 re-parsed the
   * catalogues some 7,000 times — en and de merged in `t`, plus de again in
   * `book_name` — and cost 686 ms against 9 ms with the tables shared. A reader
   * felt it as the word TAP rather than the page: the tap RPC queues behind the
   * layout on the single engine thread, so tapping a German word answered in
   * ~480 ms and read as a broken study panel (UAT, 2026-08-03).
   *
   * German because that is where it was 4× worst and where a reader reported it.
   * Psalm 119 because the cost was per word and this is the longest chapter in the
   * canon. But the assertion is NOT a comparison against English, and that is the
   * whole design of this test: the first draft compared the two languages, and it
   * PASSED against the very bug it describes, because the defect made English slow
   * too (207 ms) and the ratio stayed inside the threshold. A cross-language ratio
   * cannot see a cost both languages pay.
   *
   * So the unit is the same chapter's SECOND layout, which the worker's turn cache
   * answers: identical display list, identical postMessage, none of the work. That
   * makes the budget "producing this list may cost N times handing it over", which
   * every machine sets for itself — no millisecond constant to be wrong about on a
   * loaded CI box, and no way for a device that is uniformly slow to look healthy.
   *
   * MUTATION: in `crates/core/src/i18n.rs`, make `t()` call `resolved(lang)` again
   * and `book_name()` call `catalog(lang)`, restoring the per-call parse. Red at
   * ~340× against the 40× allowed. `a_catalogue_is_parsed_once_and_shared` in that
   * same file is the deterministic half of this pair.
   */
  test("does not redo work per word", { tag: "@perf" }, async ({ page }) => {
    await reader(page, EN);
    await pick(page, EN, "Deutsch");
    // Past the background load, so the cold measurement is not queued behind it.
    await page.waitForTimeout(6000);
    const [cold, transport] = await layoutCostAndFloor(page, "Ps", 119, 902);

    expect(
      cold / transport,
      `Psalm 119 took ${cold.toFixed(0)} ms to lay out and ${transport.toFixed(1)} ms to hand over again ` +
        `(${(cold / transport).toFixed(0)}× the transport). A chapter is ~2,300 words; a ratio this large ` +
        `means per-word work that should have been done once.`,
    ).toBeLessThan(40);
  });
});

// MUTATION: build-web-pack.mjs — give the German corpus `stage: "text"`. Red
// here (and check-web-pack.mjs refuses the pack outright, which is the real
// guard; this is the behavioural half).
test.describe("an English reader", () => {
  test.use({ locale: "en-US" });

  test("never downloads the German Bible", async ({ page }) => {
    const asked: string[] = [];
    page.on("request", (r) => {
      if (r.url().includes("luther1912")) asked.push(r.url());
    });
    await reader(page, EN);
    // Past first text, and past the idle work that sweeps caches and checks for
    // updates — the sweep is the other thing that could pull an optional file.
    await page.waitForTimeout(2500);
    expect(asked, `an English reader fetched the German corpus: ${asked.join(", ")}`).toEqual([]);
  });
});
