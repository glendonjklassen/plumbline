import { expect, test, type Page } from "@playwright/test";

// PER-PANE TEXT LANGUAGE (docs/PER-PANE-LANGUAGE.md): German beside English,
// without the UI language moving.
//
// The reader's own data is shared because every text sits at the KJV's verse
// addresses, so what these tests watch for is the two ways that can go wrong:
// a pane painting the WRONG text (the turn cache serving English geometry to a
// German pane), and study answered against the wrong Bible.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page).toHaveTitle("Plumbline Bible");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".subtitle")).toHaveText(/\w+ \d+/, { timeout: 90_000 });
}

/** The words a pane actually painted, from the accessibility mirror the canvas
 *  keeps (ReaderPane's `mirror`) — the real text, not the display list.
 *
 *  `textContent`, not `innerText`: the mirror is visually hidden (1px +
 *  clip-path) so it can be read by assistive tech without being seen, and
 *  `innerText` answers with what is RENDERED, which for that box is nothing. */
async function paneText(page: Page, idx: number): Promise<string> {
  const raw = await page.locator(".pane").nth(idx).locator(".mirror").textContent();
  return (raw ?? "").replace(/\s+/g, " ").trim();
}

/**
 * Point pane `idx` at a language by its Bible's name ("Luther"), and WAIT for
 * the pane to actually be reading it.
 *
 * Waiting on the pane's own state rather than on its words: picking a language
 * is asynchronous (a download, then a corpus open), so a helper that returns on
 * the click lets the test tap a word in the English text it has not replaced
 * yet — which is how the first version of the study test below passed while
 * proving nothing.
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

// MUTATION: drop `${m.lang ?? ""}|` from the turn-cache key in
// engine.worker.ts. Red: the German pane paints the English chapter — the
// right geometry for the wrong Bible — because both panes ask for John 3 at
// the same width and the first answer is cached under a key that cannot tell
// them apart.
test("a pane reads German beside an English one, and the UI stays English", async ({ page }) => {
  await boot(page);

  // Two panes on the same chapter, so the ONLY difference between them is the
  // text — which is what makes a wrong answer visible.
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    if (s.panes.length < 2) s.addPane(0);
  });
  await expect(page.locator(".pane")).toHaveCount(2);
  // POLLED, not read once: splitting re-renders the pane row (every column
  // changes width), so both panes are briefly between display lists.
  await expect.poll(async () => (await paneText(page, 0)).length, { timeout: 30_000 }).toBeGreaterThan(20);
  const english = await paneText(page, 0);
  expect(english).toContain("Pharisees");

  await setPaneBible(page, 1, "Luther", "de");
  expect(await paneText(page, 1)).toMatch(/Pharisäern|Nikodemus|Gott/);

  const german = await paneText(page, 1);
  expect(german, "the two panes must not be the same text").not.toBe(english);
  // The pane beside it never changed — no reload, no re-language of the app.
  expect(await paneText(page, 0)).toBe(english);
  // THE INTERFACE IS UNTOUCHED: the reader picked a Bible for one column, not
  // a language for the app.
  await expect(page.getByRole("button", { name: "Study" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.lang)).not.toBe("de");
});

// FULL STUDY PER PANE: a word tapped in the German column is studied in German.
//
// The assertion has to be sensitive to the LANGUAGE, not to the position. An
// earlier version tapped the same coordinate before and after the switch and
// compared the two answers — which differ either way, because changing the text
// moves the words, so the tap lands on a different token and the study differs
// even when the language is dropped entirely. It passed against its own bug.
//
// So this pins the two links of the chain separately:
//   1. the TAP carries the pane's language into the panel view, and
//   2. the PANEL asks that language — same verse, same token, two languages,
//      two answers.
//
// MUTATION (1): drop `pane.lang` from ReaderPane's `onWordStudy` call → the
// panel view has no language. MUTATION (2): pass `undefined` instead of
// `p.lang` in StudyPanel's wordStudy `qIn` → the two answers are identical.
test("word study on a pane comes from that pane's own text", async ({ page }) => {
  await boot(page);
  await setPaneBible(page, 0, "Luther", "de");

  // ── 1. the tap carries the language ──
  const canvas = page.locator(".pane canvas").first();
  const box = (await canvas.boundingBox())!;
  for (const x of [0.3, 0.35, 0.4, 0.45, 0.5, 0.55]) {
    await canvas.click({ position: { x: box.width * x, y: 46 } });
    if (await page.evaluate(() => (window as any).__plumbline.panel?.kind === "wordStudy")) break;
  }
  const view = await page.evaluate(() => (window as any).__plumbline.panel);
  expect(view?.kind, "a tap on a word opens its study").toBe("wordStudy");
  expect(view?.lang, "and the study belongs to the pane's own text").toBe("de");

  // ── 2. the panel asks that language ──
  // The SAME verse and the SAME token, answered twice: once as the pane asked
  // (German) and once as the reader's own text. A panel that ignores the view's
  // language cannot tell these apart, and they come back identical.
  const answers = await page.evaluate(async (v: any) => {
    const s = (window as any).__plumbline;
    const de = await s.fetchQIn("de", "wordStudyBlocks", v.refKey, v.tokenIndex, s.gates);
    const en = await s.fetchQIn(undefined, "wordStudyBlocks", v.refKey, v.tokenIndex, s.gates);
    return { de: JSON.stringify(de), en: JSON.stringify(en) };
  }, view);
  expect(answers.de, "the German text must answer differently from the KJV").not.toBe(answers.en);

  // And what the panel actually PAINTS follows the view's language: the same
  // verse and token rendered through the panel twice, once with the pane's
  // language and once without, must not paint the same words.
  //
  // (The panel's REFERENCE line stays in the interface language — "John 3:1",
  // not "Johannes" — because the reader's UI is still English. That is why this
  // compares two rendered panels rather than hunting for a German word in one:
  // the first version of this check picked "John" out of the reference and
  // failed a correct app.)
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

// The cost of the thing, measured rather than assumed: three languages open at
// once is the ceiling (the web caps at three panes), and this is a wasm heap in
// ONE worker thread. Not a budget that fails the build — a number printed into
// the run so a regression is visible and a phone's limits can be argued about
// with data.
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
  // All three panes are still readable — the point of the measurement is that
  // nothing fell over, not the number itself.
  for (let i = 0; i < 3; i++) expect((await paneText(page, i)).length).toBeGreaterThan(20);
});

// A Bible no pane reads is handed back. Each open text costs its cache in the
// engine's heap (~70 MB, measured above), so a reader who tries German and goes
// back to English must not keep paying for it for the rest of the session.
//
// MUTATION: drop the `releaseUnusedLangs()` call from `setPaneLang` → the
// engine is still open after the pane returns to the KJV, and the read below
// succeeds instead of being refused.
test("a language no pane reads is released", async ({ page }) => {
  await boot(page);
  await setPaneBible(page, 0, "Luther", "de");

  // Back to the reader's own text: nothing is reading German now.
  await page.evaluate(() => (window as any).__plumbline.setPaneLang(0, ""));
  await expect
    .poll(async () => await page.evaluate(() => (window as any).__plumbline.panes[0]?.lang ?? ""), { timeout: 30_000 })
    .toBe("");

  // The engine is GONE, not merely unused: a read against it is refused rather
  // than silently answered from the KJV, which is the invariant the whole
  // per-pane path rests on.
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

  // And asking for it again just works — releasing is not a one-way door.
  await setPaneBible(page, 0, "Luther", "de");
  expect(await paneText(page, 0)).toMatch(/Pharisäern|Nikodemus|Gott/);
});
