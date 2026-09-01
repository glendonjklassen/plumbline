import { expect, test, type Page } from "@playwright/test";

// A reader looking at a verse can hand that verse to someone. What these tests hold is
// not the button but the URL:
//
//   1. it points at the hosted PWA, not at whatever origin the reader is on;
//   2. `?at=` carries the refKey whole, in the frozen compact form — the only form
//      `sharedAtRef` accepts and the only one `go:` can route;
//   3. the reader's church rides along;
//   4. arriving at that URL lands on that exact verse.
//
// (4) earns the file: a hand-built URL, a display name ("1 John 3:16") instead of a
// refKey, a mis-named parameter or a relative link all produce a string that still looks
// like a share link and opens nowhere in particular.
//
// Two stubs, the only unreal things here. `navigator.share` is undefined in headless
// Chromium and raises a native OS sheet on the platforms that have it, so it is replaced
// by a recorder proving one thing — which URL the action hands to the platform — and
// that URL is then checked by arriving at it. `navigator.clipboard.writeText` is
// recorded rather than granting clipboard permissions.

async function boot(page: Page, url = "/"): Promise<void> {
  await page.goto(url);
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Where the active pane is pointing. */
async function where(page: Page): Promise<{ book: string; chapter: number; verse: number | null }> {
  return await page.evaluate(() => {
    const s = (window as any).__plumbline;
    const p = s.panes[s.activePane];
    return { book: p.book, chapter: p.chapter, verse: p.targetVerse };
  });
}

/** Record what the action hands to the platform share sheet. Installed before
 *  boot so the property is there whenever the reader gets to press the button. */
async function recordShares(page: Page): Promise<void> {
  await page.addInitScript(() => {
    (window as any).__shared = [];
    Object.defineProperty(navigator, "share", {
      configurable: true,
      value: async (data: unknown) => void (window as any).__shared.push(data),
    });
  });
}

/** A device with no share sheet — every desktop browser on Linux, and what the clipboard
 *  fallback exists for. Made explicit rather than relied on: a test that passes only
 *  because the harness lacks the API stops meaning anything the day it gains it. */
async function noShareSheet(page: Page): Promise<void> {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "share", { configurable: true, value: undefined });
  });
}

/** Open the verse menu on `refKey`, driven through session state: a long-press has to
 *  land on a word rectangle inside the canvas, and this file is about the link, not
 *  about hit-testing. */
async function openVerseMenu(page: Page, refKey: string): Promise<void> {
  await page.evaluate((ref) => {
    (window as any).__plumbline.contextMenu = { x: 40, y: 180, refKey: ref };
  }, refKey);
  // `.menu` is also the header's utilities menu, so pin the one under test by the verse
  // it names — the display name, while `?at=` below gets the refKey.
  await expect(page.locator(".menu .ref")).toHaveText(SHOWN);
}

/** Press the verse menu's share action. The explicit timeouts matter: a bare
 *  `locator.click()` on a button that is not there waits out the whole test timeout,
 *  turning "the action is missing" into silent minutes instead of a named assertion. */
async function clickShareLink(page: Page): Promise<void> {
  const button = page.locator(".menu").getByRole("button", { name: "Share link" });
  await expect(button, "the verse menu gives a reader no way to send the verse").toBeVisible({ timeout: 15_000 });
  await button.click({ timeout: 20_000 });
  // Sharing closes the menu, like every other action in it.
  await expect(page.locator(".menu")).toHaveCount(0);
}

// A verse far from where the app lands on its own (John 3), so "it opened at the shared
// verse" cannot be satisfied by the reader having been there already.
const REF = "Isa 53:5";
/** The same verse as the reader is shown it: every string a person reads here — menu
 *  heading, share title, fallback toast — is this one; every string the machine reads
 *  is `REF`. */
const SHOWN = "Isaiah 53:5";

test("the verse menu shares a link, and that link opens the reader at that verse", async ({ page }) => {
  await recordShares(page);
  await boot(page);
  expect(await page.evaluate(() => typeof navigator.share), "the share stub did not take").toBe("function");

  await openVerseMenu(page, REF);
  await clickShareLink(page);

  const shared: { title?: string; url?: string }[] = await page.evaluate(() => (window as any).__shared);
  expect(shared, "the platform share sheet was never handed anything").toHaveLength(1);
  expect(shared[0].title, "the share should name the verse, whatever the target does with a url").toContain(SHOWN);

  const url = new URL(shared[0].url!);
  // The display name is for the sentence, never for the link: `sharedAtRef` and `go:`
  // only take the frozen compact form.
  expect(url.search, "a display name must not reach the wire").not.toContain("Isaiah");
  expect(
    url.origin + url.pathname,
    "the link must point at the hosted PWA — a relative link is no link to anyone else",
  ).toBe("https://plumblinebible.org/");
  expect(url.searchParams.get("at"), "?at= must carry the refKey whole, in the frozen compact form").toBe(REF);
  // Only Present declares its recipient a new believer.
  expect(url.searchParams.get("start"), "an ordinary verse share is not a Present card").toBeNull();

  // The property that matters: arrive at the link the reader just handed over.
  await boot(page, `/${url.search}`);
  await expect
    .poll(() => where(page), { message: `the shared link did not open at ${REF}`, timeout: 20_000 })
    .toEqual({ book: "Isa", chapter: 53, verse: 5 });
});

test("a shared verse carries the reader's church to whoever opens it", async ({ page, browser }) => {
  await recordShares(page);
  await boot(page);
  await page.evaluate(() =>
    (window as any).__plumbline.setChurch({
      name: "Grace Bible Church",
      service: 600,
      url: "https://example.org",
    }),
  );

  await openVerseMenu(page, REF);
  await clickShareLink(page);
  const url = new URL(await page.evaluate(() => (window as any).__shared[0].url));

  // Built by `shareUrl`, so all three church fields travel; hand-rolling the query
  // string is how one of them gets dropped.
  expect(url.searchParams.get("church"), "the reader's church did not ride the shared verse").toBe(
    "Grace Bible Church",
  );
  expect(url.searchParams.get("churchService"), "when they meet was dropped from the link").toBe("600");
  expect(url.searchParams.get("churchUrl"), "the church's own link was dropped from the link").toBe(
    "https://example.org",
  );
  expect(url.searchParams.get("at"), "?at= must carry the refKey whole, in the frozen compact form").toBe(REF);

  // The receiving end, on a device that has never seen this app: a fresh context, so the
  // church it ends up with can only have come out of the link.
  const context = await browser.newContext();
  try {
    const guest = await context.newPage();
    await boot(guest, `/${url.search}`);
    const got = await guest.evaluate(() => (window as any).__plumbline.church);
    expect(got.name, "the church did not ride the link to the person who opened it").toBe("Grace Bible Church");
    expect(got.service).toBe(600);
    await expect
      .poll(() => where(guest), { message: `the shared link did not open at ${REF}`, timeout: 20_000 })
      .toEqual({ book: "Isa", chapter: 53, verse: 5 });
  } finally {
    await context.close();
  }
});

test("with no share sheet, Share link copies the link and says so", async ({ page }) => {
  await noShareSheet(page);
  await boot(page);
  expect(await page.evaluate(() => typeof navigator.share), "this test needs a device with no share sheet").toBe(
    "undefined",
  );
  await page.evaluate(() => {
    (window as any).__copied = [];
    navigator.clipboard.writeText = async (t: string) => void (window as any).__copied.push(t);
  });

  await openVerseMenu(page, REF);
  await clickShareLink(page);

  const copied: string[] = await page.evaluate(() => (window as any).__copied);
  expect(copied, "with no share sheet the link went nowhere at all").toHaveLength(1);
  expect(
    new URL(copied[0]).searchParams.get("at"),
    "?at= must carry the refKey whole, in the frozen compact form",
  ).toBe(REF);

  // The fallback has to say what happened, and name the verse: the menu it closed was
  // the only thing on screen that did.
  const toast = page.locator(".toast");
  await expect(toast, "a silent clipboard fallback tells the reader nothing").toBeVisible({ timeout: 5_000 });
  await expect(toast).toContainText(SHOWN);
});
