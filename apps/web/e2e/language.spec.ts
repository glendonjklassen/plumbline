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
  await dialog.getByRole("radio", { name: want, exact: true }).check();
  // Picking German downloads a 2.4 MB corpus before it reloads, so this waits
  // longer than a settings change normally would.
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
    await expect(destinations(page)).toContainText(DE["nav.hymnal"]);
    // Not a coincidence of similar words: these differ from the English.
    expect(DE["nav.hymnal"]).not.toBe(EN["nav.hymnal"]);
    await expect(destinations(page)).not.toContainText(EN["nav.hymnal"]);
  });

  // MUTATION: `i18n::resolve` — drop the `chosen` arm so it only ever reads the
  // device. Red here; the test above stays green, which is why there are two.
  test("a reader who picks English keeps it, device notwithstanding", async ({ page }) => {
    await reader(page, DE);
    await pick(page, DE, "English");

    await expect(destinations(page)).toContainText(EN["nav.hymnal"]);
    await expect(destinations(page)).not.toContainText(DE["nav.hymnal"]);
  });
});

test.describe("an English device", () => {
  test.use({ locale: "en-US" });

  // The mirror, and the one that catches a save that never lands: the setting
  // has to survive the reload the picker itself performs, and then a relaunch.
  test("a reader who picks German gets German, and it survives a relaunch", async ({ page }) => {
    await reader(page, EN);
    await expect(destinations(page)).toContainText(EN["nav.hymnal"]);

    await pick(page, EN, "Deutsch");
    await expect(destinations(page)).toContainText(DE["nav.hymnal"]);

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
    await expect(destinations(page)).toContainText(DE["nav.hymnal"]);
    const again = await page.evaluate(async () => {
      const s = (window as any).__plumbline;
      return (await s.rpc.call("verse", "John 3:16"))?.body ?? "";
    });
    expect(again, "the German text did not survive a relaunch").toContain("Gott");
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
