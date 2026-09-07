import { expect, test, type Page } from "@playwright/test";

// Two things a shared link must survive, found the hard way (2026-09-07: a
// Punjabi reader who had opened the app in English before tapped a link to the
// Punjabi walk and got an English Bible and no walk).
//
// 1. The reader's HISTORY. `lastLang()` is the code this device RESOLVED last
//    time, written on every boot — "en" for anyone who ever opened the app in
//    English — and stage 1 chose the corpus from it before the link's `?lang=`
//    got a say. A link may introduce the app in a language to a reader who never
//    chose one; only a reader who CHOSE keeps theirs. The first test boots once in
//    English (so the device has a resolved code) and then arrives by a Punjabi
//    link. Against the old rule the overview paints the English text — the
//    Gurmukhi assertion is what goes red.
//
// 2. The UPDATE that delivers the destination. New stock ships in the pack, and
//    a phone still serving last release's shell consumed the link, found no such
//    thread, and then offered the update — after which the address was gone. Now
//    the arriving page stashes the address and reloads into the new build, and the
//    new build honours the stash. The reload half needs a deployed newer build and
//    is reasoned rather than driven here; the second test drives the HONOURING
//    half by planting the stash a reloading page would have left. Against the old
//    App.svelte nothing reads it, the reader lands in the text, and `.present`
//    never appears.

test.setTimeout(240_000);

const PUNJABI_WALK = "ਮੁਕਤੀ ਕਿਵੇਂ ਮਿਲਦੀ ਹੈ";

async function booted(page: Page): Promise<void> {
  await expect(page).toHaveTitle("Plumbline Bible");
  await expect(page.locator(".subtitle")).toHaveText(/\S+ \d+/, { timeout: 120_000 });
}

test("a link's language picks the Bible for a reader who never chose one", async ({ page }) => {
  // A returning reader: one plain boot leaves "en" as this device's resolved code.
  await page.goto("/");
  await booted(page);
  expect(await page.evaluate(() => localStorage.getItem("plumbline.lang"))).toBe("en");
  expect(await page.evaluate(() => localStorage.getItem("plumbline.langChosen"))).toBeNull();

  // Then the link. Its language reaches BOTH the shell and the corpus: the
  // overview's first verse is Gurmukhi, not the KJV under a Punjabi chrome.
  await page.goto(`/?lang=pa&thread=${encodeURIComponent(PUNJABI_WALK)}`);
  await expect(page.locator(".present .overview")).toBeVisible({ timeout: 180_000 });
  expect(await page.evaluate(() => document.documentElement.lang)).toBe("pa");
  await expect(page.locator(".present .title")).toContainText(PUNJABI_WALK);
  const firstVerse = page.locator(".overview .entry .body").first();
  await expect(firstVerse).toHaveText(/[਀-੿]/, { timeout: 60_000 });
  // And the link has now made the choice for them, so the next boot keeps it.
  expect(await page.evaluate(() => localStorage.getItem("plumbline.langChosen"))).toBe("1");
});

test("a stashed link is honoured by the build that reloads into it", async ({ page }) => {
  await page.addInitScript(() => {
    if (!sessionStorage.getItem("plumbline.retriedLink"))
      sessionStorage.setItem("plumbline.pendingLink", "?thread=Romans%20Road");
  });
  await page.goto("/");
  await booted(page);
  await expect(page.locator(".present .title")).toContainText("Romans Road", { timeout: 60_000 });
  // Consumed, and remembered as retried, so the same address cannot reload twice.
  expect(await page.evaluate(() => sessionStorage.getItem("plumbline.pendingLink"))).toBeNull();
  expect(await page.evaluate(() => sessionStorage.getItem("plumbline.retriedLink"))).toBe("?thread=Romans%20Road");
});
