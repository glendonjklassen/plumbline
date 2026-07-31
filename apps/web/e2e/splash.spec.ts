import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { bootErrorCopy } from "../src/engine/bootError";

// THE FIRST SCREEN, and for a long time the least examined one.
//
// Written 2026-07-30 for audit item D-11, which is four separate faults wearing
// one coat:
//
//   1. `session.applyTheme()` has always stored the resolved palette in
//      localStorage "so the boot snapshot can paint before the engine exists" —
//      and nothing had read it since the boot snapshot was removed. Every launch
//      therefore painted the LIGHT theme's cream and then repainted: a dark-theme
//      reader got a full-screen flash on every single launch, warm boots
//      included.
//   2. The splash opened on "Fetching scripture data — 0%" whatever it was doing.
//      A warm boot fetches NOTHING — that is the whole promise of the depot — so
//      the app's first sentence about itself was false on the common path.
//   3. Nothing told a first-time reader what the wait was FOR, or that it was
//      once.
//   4. A boot failure printed the raw exception at the reader
//      ("data pack file …: HTTP 503"), which says nothing they can act on — while
//      being the one string a bug report actually needs.
//
// NOT RUN by the agent that wrote this file — no Playwright in that sandbox. The
// mutation recipe for each test is on the test.

/** A palette role's hex for a theme, read out of the core rather than copied.
 *
 *  index.html carries a handful of these as pre-paint defaults, and the light
 *  palette has already moved once under this app (the WCAG pass, 2026-07-29). A
 *  copied hex would have survived that silently, which is exactly the class of
 *  drift this file exists to catch. */
function coreHex(theme: "Light" | "Dark", role: string): string {
  const src = readFileSync(new URL("../../../crates/core/src/theme.rs", import.meta.url), "utf8");
  const arm = src.split(`Theme::${theme} => Palette {`)[1];
  if (!arm) throw new Error(`theme.rs has no Theme::${theme} palette arm — this test needs updating`);
  const hex = new RegExp(`\\b${role}:\\s*"(#[0-9a-f]{6})"`).exec(arm);
  if (!hex) throw new Error(`theme.rs Theme::${theme} has no ${role} hex — this test needs updating`);
  return hex[1];
}

/** `#1f1b16` → `rgb(31, 27, 22)`, which is what getComputedStyle answers with. */
function rgb(hex: string): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgb(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255})`;
}

/** The camelCase name the wire (and therefore the CSS variable) uses. */
const ROLES = ["paper", "ink", "faded", "gold", "rule", "tier_research"] as const;
const cssName = (role: string) => role.replace(/_(.)/g, (_, c: string) => c.toUpperCase());

/** Boot to the reader, taking the established path through the chooser. The
 *  analysis tiers are left alone — nothing here is about them. */
async function boot(page: Page): Promise<void> {
  await page.goto("/");
  const established = page.getByRole("button", { name: "Established believer" });
  await expect(established.or(page.locator(".pane canvas").first())).toBeVisible({ timeout: 90_000 });
  if (await established.isVisible().catch(() => false)) {
    await established.click();
    await page.getByRole("button", { name: "Start reading" }).click();
  }
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
}

/** Replace the engine worker, for every load from here on, with a stub that
 *  either never answers or fails the boot RPC with `raw`.
 *
 *  A STUB AND NOT `page.route`. Two reasons, both of which have bitten this
 *  suite: the boot fetches all happen inside the engine worker, where
 *  page-level interception cannot see them at all (network.spec.ts), and by the
 *  second visit the service worker is serving the worker's own script out of the
 *  cache, so a route on its URL never fires. Being the worker is the only way to
 *  hold the splash still. */
async function stubEngine(page: Page, raw: string | null): Promise<void> {
  await page.addInitScript((message: string | null) => {
    const Real = window.Worker;
    const src =
      message === null
        ? "self.onmessage = () => {};"
        : "self.onmessage = (e) => { if (e.data && e.data.op === 'boot') " +
          "self.postMessage({ id: e.data.id, error: " +
          JSON.stringify(message) +
          " }); };";
    class Stub extends Real {
      constructor(_url: string | URL, opts?: WorkerOptions) {
        super(URL.createObjectURL(new Blob([src], { type: "text/javascript" })), opts);
      }
    }
    (window as any).Worker = Stub;
  }, raw);
}

/** A silent engine: the splash goes up and stays up, so `applyTheme()` can never
 *  run and anything the page is painted in came from index.html's head and
 *  nowhere else. The only way to measure a pre-engine paint without racing it. */
const noEngine = (page: Page) => stubEngine(page, null);

// MUTATION: delete the inline <script> from index.html's head (the block that
// reads "plumbline:palette"). Red: the splash paints rgb(252, 249, 244) — the
// light default — for a reader whose theme is dark. That IS the bug: a cream
// flash on every launch.
test("the splash paints last session's palette, before any engine exists", async ({ page }) => {
  await boot(page);

  // Choose dark explicitly, so this is about the STORED palette and not about
  // the device's colour scheme (the next test covers that half).
  await page.evaluate(() => {
    const s = (window as any).__plumbline;
    s.config.theme = "dark";
    s.applyTheme();
    s.saveConfig();
  });
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("plumbline:palette")), { timeout: 15_000 })
    .toContain(coreHex("Dark", "paper"));

  // Now come back with no engine at all.
  await noEngine(page);
  await page.goto("/");
  await expect(page.locator(".splash")).toBeVisible({ timeout: 30_000 });

  // The premise: nothing mounted a session this load, so `applyTheme()` cannot
  // be what painted it. Without this the test could pass on the running app.
  expect(
    await page.evaluate(() => (window as any).__plumbline === undefined),
    "a session was created, so this measured applyTheme() and not the pre-paint read",
  ).toBe(true);

  await expect(page.locator(".splash")).toHaveCSS("background-color", rgb(coreHex("Dark", "paper")));
});

test.describe("a first-ever visit on a dark device", () => {
  test.use({ colorScheme: "dark" });

  // MUTATION: delete the `@media (prefers-color-scheme: dark)` block from
  // index.html's head. Red: rgb(252, 249, 244) — cream, on a device that asked
  // for dark and has nothing stored yet, which is every reader's first launch.
  test("opens in the scheme it asked for, with nothing stored yet", async ({ page }) => {
    await noEngine(page);
    await page.goto("/");
    await expect(page.locator(".splash")).toBeVisible({ timeout: 30_000 });
    expect(
      await page.evaluate(() => localStorage.getItem("plumbline:palette")),
      "this device already had a stored palette, so it is not testing a first visit",
    ).toBeNull();
    await expect(page.locator(".splash")).toHaveCSS("background-color", rgb(coreHex("Dark", "paper")));
  });
});

// MUTATION: change any one hex in index.html's :root block (say --gold to
// #000000). Red: names the role, the theme and both hexes.
//
// Reading the source rather than the DOM on purpose: these values exist to be
// right BEFORE the app runs, so the app's opinion of them proves nothing.
test("the pre-paint defaults in index.html are the core's own palette", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  // The head <style> only: the noscript block below it defines its own `--ns-*`
  // names and is a separate thing.
  const style = /<style>([\s\S]*?)<\/style>/.exec(html)?.[1];
  if (!style) throw new Error("index.html has no head <style> — this test needs updating");
  const [light, dark] = style.split("@media (prefers-color-scheme: dark)");
  expect(dark, "index.html's pre-paint defaults no longer honour a dark device").toBeTruthy();

  for (const [theme, block] of [
    ["Light", light],
    ["Dark", dark],
  ] as const) {
    for (const role of ROLES) {
      const want = coreHex(theme, role);
      const got = new RegExp(`--${cssName(role)}:\\s*(#[0-9a-f]{6})`).exec(block!)?.[1];
      expect(
        got,
        `index.html's ${theme.toLowerCase()} --${cssName(role)} is ${got}, but theme.rs Theme::${theme} says ${want} — ` +
          `the page would paint a shade off the app it becomes`,
      ).toBe(want);
    }
  }
});

/** Record every distinct splash phase line and download note the page ever
 *  paints. Installed before any page script, because the interesting one is the
 *  FIRST — by the time a test could poll for it, a warm boot is over. */
async function watchSplash(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const seen = new Set<string>();
    const w = window as any;
    w.__splashSaid = [];
    const sample = () => {
      for (const sel of [".splash .detail", ".splash .once", ".splash .error"]) {
        const t = document.querySelector(sel)?.textContent?.trim();
        if (t && !seen.has(t)) {
          seen.add(t);
          w.__splashSaid.push(t);
        }
      }
    };
    const start = () => {
      sample();
      new MutationObserver(sample).observe(document.documentElement, {
        subtree: true,
        childList: true,
        characterData: true,
      });
    };
    // `readystatechange` fires on the DOCUMENT and does not bubble, so the
    // obvious `addEventListener` — which is `window`'s — never runs and the
    // observer is never installed. That is not a hypothetical: the first version
    // of this file did exactly that and the watcher recorded nothing at all on
    // either boot. The control assertion below is what caught it.
    if (document.documentElement) start();
    else document.addEventListener("readystatechange", start, { once: true });
  });
}

// MUTATION (either half, both are the same bug): in engine/boot.ts, put
// `onPhase({ phase: "download", fraction: 0 })` back above the ladder; or in
// App.svelte, initialise `phase` to `{ phase: "download", fraction: 0 }`. Red on
// the warm half: "a warm boot opened by claiming to be fetching" — the launch
// that downloads nothing is the one that said it was downloading.
test("a cold boot says what it is downloading and why; a warm boot claims nothing", async ({ page }) => {
  await watchSplash(page);
  await boot(page);

  const cold: string[] = await page.evaluate(() => (window as any).__splashSaid);
  // The control: if the observer never caught anything, the warm assertion below
  // would pass by measuring nothing at all.
  expect(
    cold.some((t) => t.startsWith("Fetching scripture data")),
    `the cold boot never reported a download — the watcher is not working. Saw: ${JSON.stringify(cold)}`,
  ).toBe(true);
  // Dictated copy (audit D-11), verbatim.
  expect(
    cold,
    "a first visit never said what the wait costs or that it is one-time",
  ).toContain("≈3 MB, one time — then Plumbline works with no connection");

  // Same device, second launch. Every byte is already here. The watcher does not
  // need re-installing and must not be: an init script runs on every navigation,
  // and a new document is a new window, so `__splashSaid` starts empty again all
  // by itself — installing a second copy would only double every line.
  await page.goto("/");
  await expect(page.locator(".pane canvas").first()).toBeVisible({ timeout: 90_000 });
  const warm: string[] = await page.evaluate(() => (window as any).__splashSaid);

  expect(warm.length, "the warm boot's splash said nothing at all — the watcher is not working").toBeGreaterThan(0);
  expect(
    warm[0],
    `a warm boot opened by saying "${warm[0]}" — it downloads nothing, so it must not claim to be fetching`,
  ).toBe("Preparing the study engine…");
  expect(
    warm.filter((t) => t.startsWith("Fetching scripture data")),
    "a warm boot reported a download it never made",
  ).toEqual([]);
  expect(
    warm.filter((t) => t.startsWith("≈3 MB")),
    "a warm boot billed the reader for a download that already happened",
  ).toEqual([]);
});

/** Every bucket of the boot-error mapper, and the two rules that hold it
 *  together: an already-human message passes through untouched, and anything
 *  machine-shaped is replaced rather than merely decorated.
 *
 *  A PURE test in a Playwright file, with no `page` fixture, so it costs no
 *  browser: `engine/bootError.ts` imports nothing and touches no DOM, and this
 *  shell has no other unit runner. The browser half — that the splash actually
 *  calls this, and keeps the raw — is the test below. */
const CASES: [raw: string, expect: RegExp][] = [
  // Passed through: worker-client.ts already writes these for a reader, and
  // e2e/worker-death.spec.ts asserts the splash still shows them verbatim.
  ["The study engine stopped unexpectedly — no reason given.", /^The study engine stopped unexpectedly/],
  ["The study engine went quiet for 60s and never finished starting.", /^The study engine went quiet/],
  ["data pack format 3 — this build understands 2. Rebuild the pack", /older than the scripture data/],
  ["QuotaExceededError: Failed to execute 'put' on 'Cache'", /no room left on this device/],
  ["SecurityError: The operation is insecure.", /not letting Plumbline store/],
  ["data pack file data/kjv.jsonl.idxcache: HTTP 503", /could not finish downloading/],
  // The ENGINE BINARY failing to download lands in the same bucket as the pack,
  // which is why that bucket's sentence must not name a payload: this case is
  // what boot-overlap.spec.ts drives with a real 503, and an earlier draft told
  // the reader the "scripture data" had failed when it was the wasm.
  ["plumbline_ffi.wasm: HTTP 503", /could not finish downloading what it needs to open/],
  ["plumbline_ffi.wasm: HTTP 503", /^(?!.*scripture data).*$/],
  ["TypeError: Failed to fetch", /could not finish downloading/], // chromium
  ["TypeError: Load failed", /could not finish downloading/], // WebKit
  ["CompileError: WebAssembly.instantiate(): expected magic word", /engine would not start/],
  ["engine open failed", /could not read the scripture data/],
  ["something nobody has ever seen", /could not start\./],
  ["", /could not start\./],
];

// MUTATION: in bootError.ts, change the pass-through rule's `say: ""` to any
// string. Red: the first two cases, naming the raw and what came back. Or break
// one bucket's regex (`/Failed to fetch/` → `/XFailed to fetch/`) and that
// bucket's cases fall through to the catch-all sentence.
test("every boot failure maps to something a reader can act on", () => {
  for (const [raw, want] of CASES) {
    expect(bootErrorCopy(raw), `bootErrorCopy(${JSON.stringify(raw)})`).toMatch(want);
  }
  // The whole point of the mapper: a machine string is REPLACED, not framed.
  expect(bootErrorCopy("TypeError: Failed to fetch")).not.toContain("TypeError");
});

// MUTATIONS, one per half:
//   a) App.svelte — `{bootErrorCopy(error)}` → `{error}`. Red: "the reader was
//      shown the raw exception" (the .error line contains "HTTP 503").
//   b) App.svelte — delete the <details> block under the Retry button. Red: "the
//      raw string is not reachable anywhere on the error screen" — which is what
//      a bug report has to paste.
test("a boot failure says something a reader can act on, and keeps the raw string", async ({ page }) => {
  const RAW = "data pack file data/kjv.jsonl.idxcache: HTTP 503";
  await stubEngine(page, RAW);
  await page.goto("/");

  const line = page.locator(".splash .error");
  await expect(line).toBeVisible({ timeout: 30_000 });
  await expect(line, "the boot error is not written for the reader").toContainText(
    /could not finish downloading what it needs to open/i,
  );
  expect(
    await line.textContent(),
    "the reader was shown the raw exception instead of something they can act on",
  ).not.toContain("HTTP 503");

  // And it is an error SCREEN — no progress bar still pretending.
  await expect(page.locator(".splash .bar")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();

  // The raw string, one disclosure away. Closed to begin with; the <pre> is in
  // the DOM either way, so assert the text and then that it opens.
  const raw = page.locator(".splash details pre");
  expect(
    (await raw.textContent()) ?? "",
    "the raw string is not reachable anywhere on the error screen — a bug report has nothing to paste",
  ).toContain(RAW);
  await page.locator(".splash details summary").click();
  await expect(raw).toBeVisible();
});
