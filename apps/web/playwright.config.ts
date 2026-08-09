import { defineConfig, webkit } from "@playwright/test";
import { existsSync } from "node:fs";

// E2E against the production build (vite preview). Locally, point at the
// system browser with CHROMIUM_BIN=/usr/bin/chromium; CI installs
// playwright's chromium. Each test gets a fresh storage state, so the app
// boots first-run (IndexedDB empty) unless a test seeds otherwise.
//
// TWO ENGINES, DELIBERATELY LOPSIDED (2026-07-29). Chromium runs everything.
// WebKit runs the offline promise and nothing else: it is the engine where the
// Cache API, eviction and the storage budget actually differ, and on iOS it is
// the only engine there is — so the invariant the depot exists to protect was
// least proven exactly where it was most likely to break. It is not the whole
// suite twice: every one of these tests boots the engine and downloads the
// pack, so a second full pass would cost minutes to re-assert things that
// have nothing to do with the engine.

/** The offline promise, on the engine most likely to break it.
 *
 *  Selected by TITLE, because a per-project selector is all Playwright has:
 *  rename one of these tests and it quietly stops running on WebKit. Keep the
 *  two in step. Measured 2026-07-29 against playwright's WebKit 26.5 — all five
 *  pass, ~50 s for the set.
 *
 *  Why each:
 *    - "a warm boot never asks the network …" — the sharpest of them. Zero
 *      requests before text means every byte came out of WebKit's own Cache API,
 *      not off the wire.
 *    - "the whole shell is stored after one visit …" — the shell is complete on
 *      the device, not merely whatever this page happened to load.
 *    - "Settings can make the app completely offline …" — every pack file really
 *      is there, which is the claim a tighter storage budget falsifies quietly.
 *    - "checking for an update cannot poison the cached shell" — added
 *      2026-07-30, and it is here because it FAILED here. WebKit reported the
 *      document as cached while chromium passed, and WebKit was right: mayCache()
 *      recognised "index.html asked for as data" by comparing whole URLs, so any
 *      query string walked past it. Chromium cached it too and passed only
 *      because the page read the cache before the worker's un-awaited put landed.
 *      The comparison is by pathname now and the test waits out the put — this is
 *      the engine that noticed, so this is the engine that keeps watching.
 *    - the stalled- and dead-origin boots in network.spec.ts — the document
 *      served from cache with no usable network. Their "offline" is a real socket
 *      underneath the browser rather than the browser's own emulation, which is
 *      the only kind WebKit can be tested with at all (see below).
 *
 *  One is missing on purpose, and it is a finding rather than tidying:
 *
 *    - "boots offline after ONE visit" COULD NOT run on WebKit while its offline
 *      was context.setOffline(true): Playwright's offline emulation stops WebKit
 *      consulting the service worker at all — the reload dies with "WebKit
 *      encountered an internal error" and a page fetch throws TypeError. That is
 *      the harness, not us; a minimal cache-first service worker on a throwaway
 *      origin fails identically, while chromium serves it from cache. It now kills
 *      a real origin instead (network.spec.ts), which is the fix and not a
 *      substitute — the same WebKit device booted to John 3 in 222 ms with its
 *      origin genuinely refusing connections. It stays out of this list only
 *      until someone has watched it pass here; add the title, do not assume it.
 */
const OFFLINE_ON_WEBKIT = [
  /a warm boot never asks the network for the pack or the engine/,
  /the whole shell is stored after one visit/,
  /Settings can make the app completely offline/,
  /a stalled navigation still reaches the reader/,
  /a stalled network cannot hang the boot/,
  /checking for an update cannot poison the cached shell/,
];

/** Whether playwright's WebKit was ever downloaded. `executablePath()` answers
 *  where it WOULD be without caring whether it is, so ask the disk.
 *
 *  Present is NOT the same as launchable, which is why this is only trusted
 *  under CI: playwright ships one Linux WebKit built against Ubuntu, and on any
 *  other distribution it downloads fine and then refuses to start (missing
 *  libicu74, libxml2.so.2, libflite). Gating on presence alone turned
 *  `npm run test:e2e` red on this Arch machine with five launch failures — the
 *  exact outcome this project was told not to cause. CI installs it with
 *  `--with-deps`, so there presence really does mean it runs. */
function bundledWebkit(): boolean {
  try {
    return existsSync(webkit.executablePath());
  } catch {
    return false;
  }
}

// WebKit has no system build to borrow the way chromium borrows /usr/bin/chromium
// — it is a 102 MB `npx playwright install webkit`. A config that fails
// `npm run test:e2e` for someone who has not downloaded it would be worse than
// having no WebKit project at all, so when it is absent the project simply is not
// there and a line says why. CI is the opposite case: a silent skip there is the
// coverage disappearing, so a missing browser is a loud error naming the fix.
//
// WEBKIT_BIN mirrors CHROMIUM_BIN and is not a nicety: playwright ships the
// ubuntu24.04 build, which on Arch wants libicu74, libxml2.so.2 and libflite, so
// the only way it launches is a wrapper that puts those on LD_LIBRARY_PATH. An
// explicit executablePath also skips playwright's host-requirements check, which
// otherwise refuses to launch before the browser gets a chance to work.
// So the rule is: WEBKIT_BIN runs it anywhere, and under CI the bundled build is
// trusted on its own. A developer who has merely downloaded WebKit is NOT opted
// in — downloading it must not be able to turn their suite red.
const webkitBin = process.env.WEBKIT_BIN;
const haveWebkit = !!webkitBin || (!!process.env.CI && bundledWebkit());
const howToGetWebkit = "npx playwright install webkit, then point WEBKIT_BIN at a launcher for it";
if (!haveWebkit) {
  if (process.env.CI) {
    throw new Error(`The WebKit offline project has no browser to run in: ${howToGetWebkit}`);
  }
  console.warn(
    `playwright: no WebKit here, so the offline promise is only being checked on chromium. ${howToGetWebkit}`,
  );
}

export default defineConfig({
  testDir: "./e2e",
  timeout: 120_000,
  retries: process.env.CI ? 1 : 0,
  // PARALLEL, WITH A SERIAL SECOND PASS FOR THE CLOCK-WATCHERS (2026-08-08).
  // The old `workers: 1` was justified as "one preview server" — but playwright
  // starts ONE webServer and shares it across every worker, so the server was
  // never a reason. The real limits are memory (each worker is a browser
  // booting the wasm engine) and CPU contention skewing the handful of tests
  // that assert wall-clock budgets. So: memory bounds the worker count below,
  // and the wall-clock tests are tagged @perf and run in a second, serialised
  // invocation (`npm run test:e2e`) so they measure an uncontended machine.
  // Everything else — counts, ratios, orderings, presence — parallelises
  // safely: every test boots from a fresh profile, and the specs that spin
  // their own origins all listen(0) on ephemeral ports.
  //
  // fullyParallel matters as much as the count: without it a FILE is the unit
  // of distribution, and app.spec.ts alone holds ~50 of ~230 tests — one
  // worker grinding it serially would stay the long pole regardless.
  fullyParallel: true,
  workers: process.env.CI ? 3 : 2,
  use: {
    baseURL: "http://localhost:4173",
  },
  projects: [
    {
      name: "chromium",
      use: {
        browserName: "chromium",
        launchOptions: { executablePath: process.env.CHROMIUM_BIN || undefined },
      },
    },
    ...(haveWebkit
      ? [
          {
            name: "webkit-offline",
            grep: OFFLINE_ON_WEBKIT,
            use: {
              browserName: "webkit" as const,
              launchOptions: { executablePath: webkitBin || undefined },
            },
          },
        ]
      : []),
  ],
  webServer: {
    command: "npm run preview",
    url: "http://localhost:4173",
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
