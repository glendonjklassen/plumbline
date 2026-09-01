import { expect, test, type Page } from "@playwright/test";

// Per-pane text language: German beside English, without the UI language moving.
// Every text sits at the KJV's verse addresses, so the two ways this goes wrong are a
// pane painting the wrong text (the turn cache serving English geometry to a German
// pane) and study answered against the wrong Bible.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** The words a pane actually painted, from the canvas's accessibility mirror.
 *
 *  `textContent`, not `innerText`: the mirror is visually hidden (1px + clip-path), and
 *  `innerText` answers with what is rendered, which for that box is nothing. */
async function paneText(page: Page, idx: number): Promise<string> {
  const raw = await page.locator(".pane").nth(idx).locator(".mirror").textContent();
  return (raw ?? "").replace(/\s+/g, " ").trim();
}

/**
 * Point pane `idx` at a language by its Bible's name ("Luther"), and wait on the pane's
 * own state until it is reading it. Picking a language is asynchronous (a download, then
 * a corpus open), so a helper that returned on the click would let a test tap a word in
 * the English text that has not been replaced yet.
 */
async function setPaneBible(page: Page, idx: number, bible: string, code: string): Promise<void> {
  const pane = page.locator(".pane").nth(idx);
  await pane.locator("button.lang").click();
  await pane.getByRole("menuitem").filter({ hasText: bible }).click();
  await expect
    .poll(async () => await page.evaluate((i) => (window as any).__plumbline.panes[i]?.lang ?? "", idx), {
      timeout: 180_000,
    })
    .toBe(code);
  await expect
    .poll(async () => (await paneText(page, idx)).length, { timeout: 60_000 })
    .toBeGreaterThan(20);
}

// Fails against a turn-cache key in engine.worker.ts that omits the pane's language:
// both panes ask for John 3 at the same width, so the German pane paints the cached
// English chapter.
test("a pane reads German beside an English one, and the UI stays English", async ({ page }) => {
  await boot(page);

  // Two panes on the same chapter, so the only difference between them is the text.
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    if (s.panes.length < 2) s.addPane(0);
  });
  await expect(page.locator(".pane")).toHaveCount(2);
  // Polled, not read once: splitting re-renders the pane row (every column changes
  // width), so both panes are briefly between display lists.
  await expect.poll(async () => (await paneText(page, 0)).length, { timeout: 30_000 }).toBeGreaterThan(20);
  const english = await paneText(page, 0);
  expect(english).toContain("Pharisees");

  await setPaneBible(page, 1, "Luther", "de");
  expect(await paneText(page, 1)).toMatch(/Pharisäern|Nikodemus|Gott/);

  const german = await paneText(page, 1);
  expect(german, "the two panes must not be the same text").not.toBe(english);
  // The pane beside it never changed: no reload, no re-language of the app.
  expect(await paneText(page, 0)).toBe(english);
  // The reader picked a Bible for one column, not a language for the app.
  await expect(page.getByRole("button", { name: "Study" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.lang)).not.toBe("de");
});

// A word tapped in the German column is studied in German. The assertion must be
// sensitive to the language, not the position: comparing one coordinate's answers
// before and after the switch proves nothing, because changing the text moves the words
// and the tap lands on a different token either way. So the chain is pinned in two
// places — the tap carries the pane's language into the panel view, and the panel asks
// that language.
//
// Fails against a ReaderPane `onWordStudy` call with no `pane.lang` (the view has no
// language), or a StudyPanel wordStudy `qIn` passing `undefined` (the two answers are
// identical).
test("word study on a pane comes from that pane's own text", async ({ page }) => {
  await boot(page);
  await setPaneBible(page, 0, "Luther", "de");

  // ── 1. the tap carries the language ──
  const canvas = page.locator(".pane canvas").first();
  const box = (await canvas.boundingBox())!;
  for (const x of [0.3, 0.35, 0.4, 0.45, 0.5, 0.55]) {
    await canvas.click({ position: { x: box.width * x, y: 46 } });
    if (await page.evaluate(() => (window as any).__plumbline.panel?.kind === "wordUsage")) break;
  }
  const view = await page.evaluate(() => (window as any).__plumbline.panel);
  // The tap answer is the word-usage card; the language-carrying seam is the same
  // either way.
  expect(view?.kind, "a tap on a word opens its study").toBe("wordUsage");
  expect(view?.lang, "and the study belongs to the pane's own text").toBe("de");

  // ── 2. the panel asks that language ──
  // The same verse and token, answered twice: once as the pane asked (German) and once
  // as the reader's own text. A panel that ignores the view's language returns both
  // answers identical.
  const answers = await page.evaluate(async (v: any) => {
    const s = (window as any).__plumbline;
    const de = await s.fetchQIn("de", "wordStudyBlocks", v.refKey, v.tokenIndex, s.gates);
    const en = await s.fetchQIn(undefined, "wordStudyBlocks", v.refKey, v.tokenIndex, s.gates);
    return { de: JSON.stringify(de), en: JSON.stringify(en) };
  }, view);
  expect(answers.de, "the German text must answer differently from the KJV").not.toBe(answers.en);

  // What the panel paints follows the view's language. Its reference line stays in the
  // interface language ("John 3:1", not "Johannes"), so this compares two rendered
  // panels rather than hunting for a German word in one.
  const paint = async (lang: string | undefined): Promise<string> => {
    await page.evaluate(
      (v: any) => {
        (window as any).__plumbline.panel = v;
      },
      { kind: "wordStudy", refKey: view.refKey, tokenIndex: view.tokenIndex, lang },
    );
    const panel = page.locator("aside.panel, [data-surface='study panel']").first();
    await expect(panel).toBeVisible({ timeout: 30_000 });
    await expect.poll(async () => ((await panel.textContent()) ?? "").length, { timeout: 30_000 }).toBeGreaterThan(40);
    return ((await panel.textContent()) ?? "").replace(/\s+/g, " ").trim();
  };
  const paintedDe = await paint("de");
  const paintedEn = await paint(undefined);
  expect(paintedDe, "the panel must paint the pane's own text, not the KJV").not.toBe(paintedEn);
});

// Three languages open at once is the ceiling (the web caps at three panes), on one
// wasm heap in one worker thread. This prints the measured cost rather than enforcing a
// millisecond budget; the assertion is only that nothing fell over.
test("three languages at once, and what they cost @perf", async ({ page }) => {
  test.setTimeout(300_000);
  await boot(page);

  const mb = (b: number) => Math.round((b / 1048576) * 10) / 10;
  const heap = () => page.evaluate(() => (window as any).__plumbline.rpc.wasmMemoryBytes());

  const before = await heap();
  for (const [i, bible, code] of [
    [0, "Luther", "de"],
    [1, "Reina-Valera", "es"],
  ] as const) {
    await page.evaluate((n) => {
      const s = (window as any).__plumbline;
      while (s.panes.length <= n) s.addPane(0);
    }, i + 1);
    await setPaneBible(page, i + 1, bible, code);
  }
  const after = await heap();

  console.log(
    `three texts open: wasm heap ${mb(before)} MB → ${mb(after)} MB (+${mb(after - before)} MB for two more Bibles)`,
  );
  // All three panes are still readable.
  for (let i = 0; i < 3; i++) expect((await paneText(page, i)).length).toBeGreaterThan(20);
});

// A Bible no pane reads is handed back: each open text costs ~70 MB of engine heap, so
// a reader who tries German and returns to English must not keep paying for it.
//
// Fails against a `setPaneLang` with no `releaseUnusedLangs()` call: the engine is still
// open, and the read below succeeds instead of being refused.
test("a language no pane reads is released", async ({ page }) => {
  await boot(page);
  await setPaneBible(page, 0, "Luther", "de");

  // Back to the reader's own text: nothing is reading German now.
  await page.evaluate(() => (window as any).__plumbline.setPaneLang(0, ""));
  await expect
    .poll(async () => await page.evaluate(() => (window as any).__plumbline.panes[0]?.lang ?? ""), { timeout: 30_000 })
    .toBe("");

  // The engine is gone, not merely unused: a read against it is refused rather than
  // silently answered from the KJV.
  const refused = await page.evaluate(async () => {
    try {
      await (window as any).__plumbline.rpc.callIn("de", "tocJson");
      return null;
    } catch (e) {
      return e instanceof Error ? e.message : String(e);
    }
  });
  expect(refused, "the released engine must not answer").toBeTruthy();
  expect(refused).toMatch(/not open/i);

  // Asking for it again works: releasing is not a one-way door.
  await setPaneBible(page, 0, "Luther", "de");
  expect(await paneText(page, 0)).toMatch(/Pharisäern|Nikodemus|Gott/);
});

// A pane's language survives a relaunch, engine included: `lang` rides `openPanes`, but
// the engine it names lives and dies with the worker, so boot has to reopen it.
//
// Fails against a session.svelte.ts with no restore-time `openPaneLang` loop (or that
// stops setting `langLoading` on restored panes): the restored pane's first layout
// throws "the de text is not open on this device", it sits blank, and the study read
// below is refused.
test("a restored language pane paints its text and answers study after a reload", async ({ page }) => {
  await boot(page);
  await setPaneBible(page, 0, "Luther", "de");

  // Let the debounced config write reach the worker before the reload races it.
  await page.waitForTimeout(1500);
  await page.reload();
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });

  // The pane came back German, not silently English.
  expect(await page.evaluate(() => (window as any).__plumbline.panes[0]?.lang ?? "")).toBe("de");
  await expect
    .poll(async () => await paneText(page, 0), { timeout: 60_000 })
    .toMatch(/Pharisäern|Nikodemus|Gott/);

  // And its study answers.
  const study = await page.evaluate(async () => {
    const s = (window as any).__plumbline;
    return JSON.stringify(await s.fetchQIn("de", "wordStudyBlocks", "John 3:16", 1, s.gates));
  });
  expect(study.length).toBeGreaterThan(40);
});
