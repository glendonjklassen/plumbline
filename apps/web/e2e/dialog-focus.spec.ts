import { expect, test, type Page } from "@playwright/test";

// Every `aria-modal` dialog in this shell behaves the same way to a keyboard.
//
// A CLASS test, for the reason surfaces.spec.ts and touch-targets.spec.ts are
// class tests: this was stated nowhere and therefore held nowhere. Fourteen
// dialogs, and not one of them moved focus in, held Tab, or gave focus back —
// so a screen reader stayed parked behind whatever had just opened, and Tab
// walked the page underneath it. Escape was a single ladder on `svelte:window`
// (Shell.svelte) that drops every key coming from a field, which is why Escape
// did nothing at all while the reader was typing in one.
//
// `use:modal` (src/lib/modal.ts) is the one answer, and this sweep is the thing
// that notices when a fifteenth dialog arrives without it.
//
// WHAT IS NOT HERE: SettingsDialog's restore-failed alert. It is raised from a
// `sessionStorage` note read once at mount, so it cannot be opened from outside
// the component the way every surface below can — e2e/restore-failure.spec.ts is
// where that one lives. It is also the deliberate exception to the Escape rule
// (no close path, same as its missing backdrop dismiss), which is why it would
// not belong in this table anyway.

async function boot(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Run something on the session, without ever handing Playwright a promise that
 *  only settles when the reader answers (`askConfirm`, `askText`). */
async function onSession(page: Page, body: string): Promise<void> {
  await page.evaluate(`(async () => { const s = window.__plumbline; ${body}; })()`);
}

/** Is focus inside this surface (or on the surface box itself)? */
async function focusInside(page: Page, sel: string): Promise<boolean> {
  return await page.evaluate((s) => {
    const box = document.querySelector(s);
    const a = document.activeElement;
    return !!box && !!a && (box === a || box.contains(a));
  }, sel);
}

/** What currently has focus, named well enough to read in a failure. */
async function focusName(page: Page): Promise<string> {
  return await page.evaluate(() => {
    const a = document.activeElement as HTMLElement | null;
    if (!a) return "<nothing>";
    const label = a.getAttribute("aria-label") || (a.textContent ?? "").trim().slice(0, 30);
    return `${a.tagName.toLowerCase()}${a.className ? "." + String(a.className).split(" ")[0] : ""}${
      label ? ` "${label}"` : ""
    }`;
  });
}

/**
 * Every `aria-modal` surface, and how to raise it.
 *
 * Driven through session state wherever it can be, so the table stays about the
 * keyboard and not about however each one happens to be reached. `sel` is what
 * the dialog box itself matches — `data-surface` where there is one, and the
 * `aria-label` otherwise, because `.dialog` and `.sheet` are shared by half a
 * dozen components and an ambiguous selector in a class guard is worse than no
 * guard (surfaces.spec.ts learnt that one the hard way).
 */
const DIALOGS: { name: string; open: string; sel: string }[] = [
  { name: "keyboard shortcuts", open: `s.showShortcuts = true`, sel: `[aria-label="Keyboard shortcuts"]` },
  { name: "the passage navigator", open: `s.bookNavFor = 0`, sel: `[aria-label="Go to a passage"]` },
  { name: "history", open: `s.showHistory = true`, sel: `[data-surface="history"]` },
  { name: "settings", open: `s.showSettings = true`, sel: `[data-surface="settings"]` },
  { name: "mark chapter read", open: `s.markReadFor = { book: "Gen", chapter: 1 }`, sel: `[data-surface="mark read"]` },
  { name: "the tag picker", open: `s.tagPickFor = "John 3:16"`, sel: `[data-surface="tag picker"]` },
  { name: "the thread picker", open: `s.threadPickFor = "John 3:16"`, sel: `[data-surface="thread picker"]` },
  { name: "the passage picker", open: `s.memorizePassageFrom = "John 3:16"`, sel: `[data-surface="passage picker"]` },
  { name: "the text prompt", open: `void s.askText("Note", "", true)`, sel: `[aria-label="Note"]` },
  {
    name: "the confirmation",
    open: `void s.askConfirm("Delete this?", "Body", "Delete")`,
    sel: `[data-surface="confirm"]`,
  },
  {
    name: "the tag weave sheet",
    // Needs a tag to convert, so it makes one first.
    open:
      `await s.author("tagAdd", "Focus sweep", "verse", "John 3:16", null, new Date().toISOString());` +
      `s.tagWeaveFor = 0`,
    sel: `[data-surface="tag weave"]`,
  },
];

// Mutations, each one line in src/lib/modal.ts, all three run against this test:
//
//   * delete the opening focus (`if (target) target.focus(); else node.focus(…)`)
//     → 'Error: opening the keyboard shortcuts left focus outside it — on body
//        expect(received).toBe(expected)  Expected: true  Received: false'.
//   * `return;` as the first line of the `Tab` half of `onKeydown` (i.e. no trap)
//     → 'Error: Tab walked out of settings, onto body "" — the page behind a
//        modal is not somewhere Tab may go'.
//   * delete `opts.close?.()` from the Escape branch
//     → 'Error: expect(locator).toHaveCount(expected) failed  Locator:
//        locator("[data-surface=\"history\"]")  Expected: 0  Received: 1'.
test("every dialog takes focus, holds Tab, and closes on Escape", async ({ page }) => {
  await boot(page);

  for (const d of DIALOGS) {
    await onSession(page, `s.dismissTransient()`);
    await onSession(page, d.open);
    const box = page.locator(d.sel);
    await expect(box, `${d.name} should open`).toBeVisible({ timeout: 20_000 });

    // 1. Focus came in. Not "a focusable exists" — where the caret actually is.
    expect(
      await focusInside(page, d.sel),
      `opening ${d.name} left focus outside it — on ${await focusName(page)}`,
    ).toBe(true);

    // 2. Tab cannot leave. A few ordinary presses from where focus opened…
    for (let i = 0; i < 5; i++) {
      await page.keyboard.press("Tab");
      if (!(await focusInside(page, d.sel))) {
        throw new Error(
          `Tab walked out of ${d.name}, onto ${await focusName(page)} — ` +
            `the page behind a modal is not somewhere Tab may go`,
        );
      }
    }
    // …and then the two presses that actually exercise the trap. Walking the
    // whole cycle instead would take 40-odd presses on the passage navigator and
    // still only prove what these two do: the ends are the ONLY place a trap
    // acts, and a sweep that stops short of them passes with no trap at all.
    const count = await box.evaluate(
      (el) =>
        Array.from(
          el.querySelectorAll<HTMLElement>(
            "a[href],button:not([disabled]),input:not([disabled]),select:not([disabled])," +
              "textarea:not([disabled]),summary,[tabindex]:not([tabindex='-1'])",
          ),
        ).filter((c) => c.getClientRects().length > 0).length,
    );
    if (count > 0) {
      await box.evaluate((el) => {
        const items = Array.from(
          el.querySelectorAll<HTMLElement>(
            "a[href],button:not([disabled]),input:not([disabled]),select:not([disabled])," +
              "textarea:not([disabled]),summary,[tabindex]:not([tabindex='-1'])",
          ),
        ).filter((c) => c.getClientRects().length > 0);
        items[items.length - 1].focus();
      });
      await page.keyboard.press("Tab");
      if (!(await focusInside(page, d.sel))) {
        throw new Error(`Tab off the END of ${d.name} escaped onto ${await focusName(page)}`);
      }
      await box.evaluate((el) => {
        const items = Array.from(
          el.querySelectorAll<HTMLElement>(
            "a[href],button:not([disabled]),input:not([disabled]),select:not([disabled])," +
              "textarea:not([disabled]),summary,[tabindex]:not([tabindex='-1'])",
          ),
        ).filter((c) => c.getClientRects().length > 0);
        items[0].focus();
      });
      await page.keyboard.press("Shift+Tab");
      if (!(await focusInside(page, d.sel))) {
        throw new Error(`Shift+Tab off the FRONT of ${d.name} escaped onto ${await focusName(page)}`);
      }
    }

    // 3. Escape closes it, from wherever the tabbing left off.
    await page.keyboard.press("Escape");
    await expect(box, `Escape did not close ${d.name}`).toHaveCount(0, { timeout: 5_000 });
  }

  await onSession(page, `s.dismissTransient()`);
});

// THE REPORTED GAP, on its own, because it is the one a reader met.
//
// Shell's Escape ladder is a `svelte:window` listener that returns early on
// `isEditable(e.target)` — so the key did nothing while the caret was in a field,
// which is exactly where a reader is when they change their mind.
//
// Mutation: in src/lib/modal.ts change `if (e.key === "Escape") {` to
//   `if (e.key === "Escape" && !(e.target instanceof HTMLInputElement)) {` —
//   i.e. put the reported bug back, and only for inputs →
//   'Error: expect(locator).toHaveCount(expected) failed  Locator:
//    locator("[data-surface=\"thread picker\"]")  Expected: 0  Received: 1'
//   while the textarea half of the test still passes, which is the shape the bug
//   actually had.
test("Escape closes a dialog from inside a text field", async ({ page }) => {
  await boot(page);

  // A single-line input: the thread picker's "New thread…" box.
  await onSession(page, `s.threadPickFor = "John 3:16"`);
  const sheet = page.locator('[data-surface="thread picker"]');
  await expect(sheet).toBeVisible({ timeout: 20_000 });
  const field = page.getByPlaceholder("New thread…");
  await field.click();
  await field.fill("half a name");
  expect(await focusName(page)).toContain("input");
  await page.keyboard.press("Escape");
  await expect(sheet).toHaveCount(0, { timeout: 5_000 });

  // A textarea, where the reader may be mid-sentence. The prompt's own contract
  // is that Escape answers null and the text is discarded (it is the same answer
  // Cancel gives, and `use:modal` routes to that same `finish(null)` rather than
  // inventing a second way out) — so what is asserted is that the promise
  // SETTLES, which is what a caller left hanging would not do.
  await page.evaluate(() => {
    (window as any).__prompt = "pending";
    void (window as any).__plumbline
      .askText("Note", "", true)
      .then((v: string | null) => ((window as any).__prompt = v === null ? "null" : `text:${v}`));
  });
  const prompt = page.locator('[aria-label="Note"]');
  await expect(prompt).toBeVisible({ timeout: 20_000 });
  // The field took focus by itself: `data-modal-focus` on the textarea, which
  // replaced a `setTimeout(…, 30)` that raced the mount.
  expect(await focusName(page), "the prompt did not put the caret in its own field").toContain(
    "textarea",
  );
  await page.keyboard.type("a sentence the reader was part-way through");
  await page.keyboard.press("Escape");
  await expect(prompt).toHaveCount(0, { timeout: 5_000 });
  expect(
    await page.evaluate(() => (window as any).__prompt),
    "Escape left the caller of askText awaiting a promise that never settled",
  ).toBe("null");
});

// Mutation: comment out the restore in modal.ts's `destroy`
//   (`if (stranded && returnTo?.isConnected) returnTo.focus();`) →
//   'Error: closing the dialog dropped focus onto body instead of returning it
//    to the control that opened it'.
test("closing a dialog gives focus back to the control that opened it", async ({ page }) => {
  // A phone, because the opener has to be a PERSISTENT control: the old Share
  // dialog moved onto the Share destination, and every remaining dialog opens
  // from the ≡ menu — which unmounts on the click, leaving nothing to restore
  // to. The phone header's passage button opens the BookNav dialog and is
  // still standing when it closes.
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page);

  const passage = page.locator("header .chapter-nav .passage");
  await passage.focus();
  await passage.press("Enter");
  const dialog = page.locator('[role="dialog"][aria-label="Go to a passage"]');
  await expect(dialog).toBeVisible({ timeout: 20_000 });
  expect(await focusInside(page, '[role="dialog"][aria-label="Go to a passage"]')).toBe(true);

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0, { timeout: 5_000 });
  expect(
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.className ?? ""),
    "closing the dialog dropped focus onto body instead of returning it to the control that opened it",
  ).toContain("passage");
});

// NESTED SURFACES. A confirmation is asked FROM another sheet, so both are on
// screen at once, and Escape has to mean "answer this one" and not "close two".
//
// Mutation: delete `e.stopPropagation()` from modal.ts's Escape branch →
//   'Error: expect(locator).toBeVisible() failed  Locator: locator("[data-surface
//    =\"thread picker\"]")' — Shell's window-level ladder also ran and peeled the
//   sheet underneath.
test("Escape in a nested confirmation closes only the confirmation", async ({ page }) => {
  await boot(page);
  await onSession(
    page,
    `await s.author("threadAdd", "Nesting test", "John 3:16", null, new Date().toISOString());` +
      `s.threadPickFor = "John 3:16"`,
  );
  const sheet = page.locator('[data-surface="thread picker"]');
  await expect(sheet).toBeVisible({ timeout: 20_000 });
  await sheet.getByTitle("Delete this thread").first().click();

  const confirm = page.locator('[data-surface="confirm"]');
  await expect(confirm).toBeVisible({ timeout: 20_000 });
  expect(
    await focusInside(page, '[data-surface="confirm"]'),
    "the confirmation did not take focus from the sheet that asked it",
  ).toBe(true);

  await page.keyboard.press("Escape");
  await expect(confirm).toHaveCount(0, { timeout: 5_000 });
  await expect(sheet, "Escape closed the sheet underneath as well").toBeVisible();
  // The thread is still there: Escape answered "no", which is what Cancel means.
  await expect(sheet.getByText("Nesting test")).toBeVisible();

  await onSession(page, `s.dismissTransient()`);
});

// The confirmation is the one dialog with a knowable tab order — two buttons —
// so it is where the WRAP itself can be asserted rather than only "focus stayed
// inside". Cancel first, and focus opens on the dialog rather than on either
// button: handing a keyboard the destructive one is not a default worth having.
//
// Mutation: in modal.ts's Tab branch, replace `first.focus()` with
//   `items[items.length - 1].focus()` → 'Error: Tab past the last control should
//    wrap to the first  expect(received).toBe(expected)  Expected: "Cancel"
//    Received: "Delete"'.
test("Tab wraps around inside a dialog", async ({ page }) => {
  await boot(page);
  await onSession(page, `void s.askConfirm("Delete this?", "Body", "Delete")`);
  const confirm = page.locator('[data-surface="confirm"]');
  await expect(confirm).toBeVisible({ timeout: 20_000 });

  const active = () => page.evaluate(() => (document.activeElement?.textContent ?? "").trim());
  expect(await page.evaluate(() => document.activeElement === document.querySelector('[data-surface="confirm"]')))
    .toBe(true);

  await page.keyboard.press("Tab");
  expect(await active()).toBe("Cancel");
  await page.keyboard.press("Tab");
  expect(await active()).toBe("Delete");
  await page.keyboard.press("Tab");
  expect(await active(), "Tab past the last control should wrap to the first").toBe("Cancel");
  await page.keyboard.press("Shift+Tab");
  expect(await active(), "Shift+Tab off the front should wrap to the last").toBe("Delete");

  await page.keyboard.press("Escape");
  await expect(confirm).toHaveCount(0, { timeout: 5_000 });
});

// A dialog can have NO focusable control at all — the history sheet with nothing
// in it is one. Focus still has to land somewhere, and Tab still must not walk
// out onto the page behind.
//
// Mutation: delete the `items.length === 0` branch from modal.ts's Tab handling
//   → 'Error: Tab escaped an empty dialog onto body  expect(received)
//      .toBe(expected)  Expected: true  Received: false'.
test("a dialog with no focusable control still holds focus", async ({ page }) => {
  await boot(page);
  await onSession(page, `s.config.history = []; s.showHistory = true`);
  const sheet = page.locator('[data-surface="history"]');
  await expect(sheet).toBeVisible({ timeout: 20_000 });
  expect(
    await sheet.evaluate((el) => el.querySelectorAll("button, a[href], input").length),
    "this test needs a dialog with nothing focusable in it",
  ).toBe(0);

  expect(await page.evaluate(() => document.activeElement === document.querySelector('[data-surface="history"]')))
    .toBe(true);
  await page.keyboard.press("Tab");
  expect(await focusInside(page, '[data-surface="history"]'), "Tab escaped an empty dialog onto body").toBe(
    true,
  );
  await page.keyboard.press("Escape");
  await expect(sheet).toHaveCount(0, { timeout: 5_000 });
});

// ── the toast ─────────────────────────────────────────────────────────────────

/** Chromium's live regions and the text each holds. Same technique and the same
 *  reasoning as a11y.spec.ts: an `aria-live` attribute on a node the tree has
 *  dropped reads perfectly in the DOM and announces nothing at all. */
async function axLive(page: Page): Promise<{ live: string; text: string }[]> {
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
    .map((n) => ({ live: String((n.properties ?? []).find((p: any) => p.name === "live").value.value), text: text(n) }));
}

// Every confirmation this app gives arrives in the toast — "Copied", "Tagged
// Isaiah 53:5", "Couldn't make the backup" — and it had no role at all, so a
// screen reader was told none of them. The update and storage notices beside it
// already carried `role="status"`; the one that speaks most often did not.
//
// Mutation: drop `role="status"` from the toast in Shell.svelte →
//   'Error: the toast is not a live region, so nothing will speak it
//    Expected: ["polite"]  Received: []' (the poll below runs out its 15 s and
//   reports the last value it saw).
//
// The toast clears itself after a few seconds, and a CDP tree dump is not
// instant, so the poll RE-SHOWS it on every attempt rather than racing that
// timer once. Nothing about the assertion is weakened by it: a toast with no
// role never appears as a live region however many times it is shown.
test("the toast is announced", async ({ page }) => {
  await boot(page);
  await onSession(page, `s.showToast("Focus sweep toast")`);
  const toast = page.locator(".toast");
  await expect(toast).toHaveText("Focus sweep toast");
  await expect(toast).toHaveAttribute("role", "status");

  await expect
    .poll(
      async () => {
        await onSession(page, `s.showToast("Focus sweep toast")`);
        return (await axLive(page)).filter((r) => r.text.includes("Focus sweep toast")).map((r) => r.live);
      },
      { timeout: 15_000, message: "the toast is not a live region, so nothing will speak it" },
    )
    .toEqual(["polite"]);
});
