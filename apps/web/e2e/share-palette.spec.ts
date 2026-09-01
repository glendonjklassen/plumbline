// The share palette: one link, built from what the recipient will actually get.
//
// The load-bearing claim is the availability one — a sender picking a language
// for somebody else must be told what does not exist in it yet — so that is what
// these drive. The engine answers per language (`plumbline_engine_share_options_json`),
// and the palette shows what is missing as coming soon rather than hiding it,
// because a sender who finds an empty list learns nothing.
//
// Each test below FAILS against a real defect rather than against a rewrite:
//  - the availability test fails if the palette ever filters unavailable rows
//    out (the list would be short and the disabled option absent), which is the
//    most tempting wrong implementation of "only offer what's available";
//  - the round-trip test fails if any parameter is dropped by the builder OR
//    ignored by the boot path, because it asserts on the app's STATE after
//    opening the link rather than on the link string it just built;
//  - the language test fails if `?lang=` reaches the interface but not the
//    corpus (or the reverse) — the two halves are asserted separately, since a
//    boot that sets one and not the other is exactly the bug worth catching.

import { expect, test, type Locator, type Page } from "@playwright/test";

/** A palette control by its row LABEL.
 *
 *  Not `select.nth(n)`: the sub-select for the chosen destination appears and
 *  disappears, so an index means different controls depending on what is
 *  selected — which is exactly how a test ends up filling the thread box with a
 *  language code. `> span:text-is()` pins the row's own label rather than
 *  matching option text inside a select ("Thread" is also an option of the
 *  Destination select). */
const field = (card: Locator, label: string) =>
  card.locator(`label.row:has(> span:text-is("${label}"))`).locator("select");

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

async function openShare(page: Page) {
  await page.getByRole("navigation").getByRole("button", { name: "Share", exact: true }).click();
  const card = page.locator('[data-surface="share custom"]');
  await expect(card).toBeVisible();
  return card;
}

/** The link the QR is actually encoding, read out of the app rather than
 *  rebuilt here — a test that rebuilds the link is a test of its own copy. */
const builtLink = (page: Page) => page.evaluate(() => (window as any).__plumbline.customShareLink as string);

test("the palette offers every language and marks what is not written yet", async ({ page }) => {
  await boot(page);
  const card = await openShare(page);

  // All nine, named in the SENDER's language — "Punjabi", not "ਪੰਜਾਬੀ". The
  // sender is naming someone else's language while reading their own, so the
  // endonym (what Settings shows, where you pick your own) is the wrong word
  // here. Plus the default, which carries no `lang=` at all and leaves the
  // recipient's own device to decide: a real option rather than a pre-filled
  // language, so a palette nobody has touched cannot quietly force the sender's
  // language onto a phone that would have picked its own.
  const langs = field(card, "Language");
  await expect(langs.locator("option")).toHaveCount(10);
  await expect(langs).toHaveValue("");
  await expect(langs.locator("option", { hasText: "Punjabi" })).toHaveCount(1);
  await expect(langs.locator("option", { hasText: "ਪੰਜਾਬੀ" })).toHaveCount(0);
  expect(await builtLink(page), "an untouched palette adds no parameters").not.toContain("lang=");

  // Aim the link at Arabic. The two PROSE paths have no Arabic welcome written,
  // so they must still be LISTED and must be disabled and labelled — a palette
  // that filtered them out would pass a "not selectable" check and lose the
  // "coming soon" the sender was promised.
  await langs.selectOption("ar");

  // The devotional is the other real gate: one booklet, English only. With the
  // link aimed at Arabic the DESTINATION itself is refused, rather than offered
  // and then backed by an empty list — a reader deciding what to send is told at
  // the point of deciding.
  const dest = field(card, "Destination");
  await expect(dest.locator('option[value="devotional"]')).toHaveAttribute("disabled", /.*/);
  await expect(dest.locator('option[value="devotional"]')).toContainText("Coming soon");
  // A thread is refs, so every corpus resolves it: that destination stays open
  // even in a language nothing else reaches, AND a thread already chosen
  // survives the language change. Both halves, because the availability answer
  // arrives a frame late — treating "not answered yet" as "not available" greyed
  // Thread out and blanked the reader's choice under them.
  await expect(dest.locator('option[value="thread"]')).not.toHaveAttribute("disabled", /.*/);
  await dest.selectOption("thread");
  await expect(field(card, "Thread")).toHaveValue("Romans Road");
  await langs.selectOption("pa");
  await expect(dest.locator('option[value="thread"]')).not.toHaveAttribute("disabled", /.*/);
  await expect(field(card, "Thread")).toHaveValue("Romans Road");

  // Back in English the booklet is reachable, and choosing the destination FILLS
  // it — "Destination: Devotional" with an empty box beneath is a question the
  // palette could have answered itself, and "none" was never one of the answers.
  await langs.selectOption("en");
  await expect(dest.locator('option[value="devotional"]')).not.toHaveAttribute("disabled", /.*/);
  await dest.selectOption("devotional");
  const booklets = field(card, "Devotional");
  await expect(booklets).not.toHaveValue("");
  await expect(booklets.locator('option[value=""]')).toHaveCount(0);
  expect(await builtLink(page), "a filled destination reaches the link").toContain("devotional=");

  // Selecting a thread fills it the same way, with no empty option to fall into.
  await dest.selectOption("thread");
  const threads = field(card, "Thread");
  await expect(threads).toHaveValue("Romans Road");
  await expect(threads.locator('option[value=""]')).toHaveCount(0);
});

test("a custom link carries the palette's choices and the recipient lands on them", async ({ page }) => {
  await boot(page);
  const card = await openShare(page);

  await page.evaluate(() =>
    (window as any).__plumbline.setChurch({ name: "Grace Bible Church", service: 600, url: "" }),
  );
  await field(card, "Destination").selectOption("thread");
  await field(card, "Thread").selectOption("Romans Road");
  await field(card, "Language").selectOption("pa");

  const link = await builtLink(page);
  expect(link).toContain("thread=Romans+Road");
  expect(link).toContain("lang=pa");
  expect(link).toContain("church=Grace+Bible+Church");

  // The readout says the same thing the link does, in words. It lives with the
  // QR — the thing it describes — not in the palette, where it would only
  // restate the controls. Scoped to it, because the thread name is also the text
  // of the option that chose it and matching either would let an empty one pass.
  const readout = page.locator(".preview");
  await expect(readout.getByText("Romans Road")).toBeVisible();
  await expect(readout.getByText("Punjabi")).toBeVisible();

  // Now BE the recipient: a fresh profile opening that link. Asserting on where
  // the app ENDS UP, not on the string, is what makes this cover the boot half —
  // a builder that emits every parameter and a boot path that ignores them all
  // would pass every assertion above.
  const recipient = await page.context().browser()!.newContext();
  const fresh = await recipient.newPage();
  await fresh.goto(`/${new URL(link).search}`);

  // Straight onto the thread the link named. Nothing in between: there is no
  // welcome any more, and the person who handed the QR over provides the context
  // it used to. This is the whole point of the destination parameters, so it is
  // asserted on a FRESH profile — the reader most likely to be handed a link is
  // the one who has never opened the app.
  await expect(fresh.locator(".present .title")).toContainText("Romans Road", { timeout: 90_000 });

  // And in Punjabi, because that is what the link said. Asserted on the CORPUS
  // the engine opened rather than on chrome text: Present is fullscreen, so
  // there is no navigation to read here, and the Bible they were handed is the
  // half that matters anyway. A boot that honoured the destination but not the
  // language passes the assertion above and still hands a Punjabi speaker an
  // English Bible.
  const corpus = await fresh.evaluate(async () => {
    const trace = await (window as any).__plumbline.rpc.bootTrace();
    return (trace as [string, number][]).map(([s]) => s).find((s) => s.startsWith("corpus loaded"));
  });
  expect(corpus, "the Punjabi link must open the Punjabi Bible").toContain("pa");

  // The link is stripped from the address bar: a reload is a reload, not a second
  // arrival, and a bookmark of this is a bookmark of the app.
  await expect.poll(() => fresh.evaluate(() => location.search)).toBe("");
  await recipient.close();
});

test("a shared language reaches BOTH the interface and the corpus", async ({ page }) => {
  // The half-applied version of this feature is the one worth catching: a boot
  // that translates the chrome over an English Bible, or opens the German text
  // under English chrome. Neither half implies the other, so both are asserted.
  // A phone, because "Read" is a nav BUTTON only in the bottom bar — on a desktop
  // it is the base layer the other roles sit on, and has no button to read.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/?lang=de");

  // The chrome is German…
  await expect(page.getByRole("navigation").getByRole("button", { name: "Lesen", exact: true })).toBeVisible({
    timeout: 90_000,
  });
  // …and so is the text the engine opened, which the boot trace names.
  const corpus = await page.evaluate(async () => {
    const t = await (window as any).__plumbline.rpc.bootTrace();
    return (t as [string, number][]).map(([s]) => s).find((s) => s.startsWith("corpus loaded"));
  });
  expect(corpus, "the German link must open the German Bible, not the KJV").toContain("de");
});

test("a preset saves, survives a reload, and restores in one click", async ({ page }) => {
  await boot(page);
  await page.evaluate(() => (window as any).__plumbline.setChurch({ name: "Grace Bible Church", service: 600, url: "" }));
  const card = await openShare(page);

  await field(card, "Destination").selectOption("thread");
  await field(card, "Thread").selectOption("Romans Road");
  await field(card, "Language").selectOption("pa");
  const built = await builtLink(page);

  await card.getByRole("button", { name: "Save Preset" }).click();
  await page.locator(".dialog input[data-modal-focus]").fill("Tuesday outreach");
  await page.locator(".dialog button.primary").click();
  await expect(card.getByRole("button", { name: "Tuesday outreach" })).toBeVisible();
  // Wait for the TOAST, not just the chip. The chip appears the moment the draft
  // is written to the config in memory; the toast is shown only after the worker
  // has the save. Reloading between the two is a race this test would otherwise
  // lose about one run in several — and it is the reader's race too, which is why
  // `savePreset` awaits the flush rather than firing and forgetting.
  await expect(page.getByText("Preset saved")).toBeVisible();

  // Through a RELOAD, which is the whole point of saving one — and the reload
  // must come back on the plain link. A palette that restored the last preset
  // would hand out a Punjabi gospel link to whoever the reader shares with next,
  // so this asserts the absence as hard as it asserts the presence.
  await page.reload();
  await boot(page);
  const card2 = await openShare(page);
  const plain = await builtLink(page);
  expect(plain, "the palette opens plain, never on a preset").not.toContain("thread=");
  expect(plain).not.toContain("lang=");
  await expect(card2.getByRole("button", { name: "Tuesday outreach" })).toBeVisible();

  // One click restores exactly what was saved.
  await card2.getByRole("button", { name: "Tuesday outreach" }).click();
  await expect.poll(() => builtLink(page)).toBe(built);

  // And deleting asks first, the way every destructive action in the app does.
  await card2.getByRole("button", { name: "Delete preset" }).click();
  // `.danger`, not `.primary`: the confirm dialog puts the destructive verb on
  // its own class, which is the shared dialog every delete in the app goes
  // through — not a bespoke one for this screen.
  await page.locator(".dialog button.danger").click();
  await expect(card2.getByRole("button", { name: "Tuesday outreach" })).toHaveCount(0);
});

test("a preset stores the choices, not the finished link", async ({ page }) => {
  // The reason this matters: a stored URL would freeze the church as it read the
  // day it was saved. Renaming the church has to change every preset's link, or
  // a reader who corrects a typo keeps handing out the old name forever.
  await boot(page);
  await page.evaluate(() => (window as any).__plumbline.setChurch({ name: "Old Name", service: null, url: "" }));
  const card = await openShare(page);
  await field(card, "Destination").selectOption("thread");
  await field(card, "Thread").selectOption("Romans Road");
  await card.getByRole("button", { name: "Save Preset" }).click();
  await page.locator(".dialog input[data-modal-focus]").fill("Outreach");
  await page.locator(".dialog button.primary").click();
  await expect(card.getByRole("button", { name: "Outreach" })).toBeVisible();

  await page.evaluate(() => (window as any).__plumbline.setChurch({ name: "New Name", service: null, url: "" }));
  await card.getByRole("button", { name: "Outreach" }).click();
  await expect.poll(() => builtLink(page)).toContain("church=New+Name");
  expect(await builtLink(page)).not.toContain("Old+Name");
});
