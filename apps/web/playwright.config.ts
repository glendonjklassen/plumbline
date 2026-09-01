import { defineConfig, webkit } from "@playwright/test";
import { existsSync } from "node:fs";

// E2E against the production build (vite preview). Locally, point at the system browser with
// CHROMIUM_BIN=/usr/bin/chromium; CI installs playwright's chromium. Each test gets a fresh
// storage state, so the app boots first-run (IndexedDB empty) unless a test seeds otherwise.
//
// Chromium runs everything; WebKit runs the offline promise and nothing else. WebKit is where the
// Cache API, eviction and the storage budget differ, and on iOS it is the only engine there is.
// Running the whole suite twice would cost minutes re-asserting things unrelated to the engine.

/** The offline promise, on the engine most likely to break it.
 *
 *  Selected by TITLE — a per-project grep is all Playwright has — so renaming one of these tests
 *  silently stops it running on WebKit. Keep the two in step.
 *
 *  Playwright's context.setOffline(true) stops WebKit consulting the service worker at all (the
 *  reload dies with an internal error), so WebKit's offline coverage has to come from tests that
 *  use a real stalled or dead origin. That is why the network.spec.ts boots are listed and "boots
 *  offline after ONE visit" is not; add its title once someone has watched it pass here.
 *
 *  "checking for an update cannot poison the cached shell" is listed because WebKit is the engine
 *  that caught it: mayCache() compared whole URLs, so a query string walked past its check. */
const OFFLINE_ON_WEBKIT = [
  /a warm boot never asks the network for the pack or the engine/,
  /the whole shell is stored after one visit/,
  /Settings can make the app completely offline/,
  /a stalled navigation still reaches the reader/,
  /a stalled network cannot hang the boot/,
  /checking for an update cannot poison the cached shell/,
];

/** Whether playwright's WebKit was ever downloaded — `executablePath()` answers where it WOULD be
 *  without caring whether it is, so ask the disk. Present is not launchable: playwright ships one
 *  Linux WebKit built against Ubuntu, and on any other distribution it downloads fine then refuses
 *  to start (missing libicu74, libxml2.so.2, libflite). Only CI, which installs it `--with-deps`,
 *  may treat presence as proof it runs. */
function bundledWebkit(): boolean {
  try {
    return existsSync(webkit.executablePath());
  } catch {
    return false;
  }
}

// WebKit has no system build to borrow the way chromium borrows /usr/bin/chromium, so when it is
// absent the project is simply not there and a line says why. Under CI a silent skip would be the
// coverage disappearing, so a missing browser is a loud error naming the fix.
//
// WEBKIT_BIN mirrors CHROMIUM_BIN and is required off Ubuntu: the shipped build wants libicu74,
// libxml2.so.2 and libflite on LD_LIBRARY_PATH, and an explicit executablePath also skips
// playwright's host-requirements check, which otherwise refuses to launch at all. Merely having
// downloaded WebKit does not opt a developer in — that must not be able to turn their suite red.
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
  // Parallel, with a serial second pass for the clock-watchers. Playwright starts one webServer
  // and shares it across workers, so the server never bounded this; memory does (each worker boots
  // the wasm engine), as does CPU contention skewing the wall-clock budgets — those tests are
  // tagged @perf and run in a second, serialised invocation (`npm run test:e2e`). Everything else
  // parallelises safely: every test boots a fresh profile and specs that spin their own origins
  // listen(0) on ephemeral ports.
  //
  // fullyParallel matters as much as the count: without it a file is the unit of distribution, and
  // app.spec.ts alone holds ~50 of ~230 tests.
  fullyParallel: true,
  // Measured, not guessed. The suite is wait-bound, not CPU-bound: 320 tests summing 425s of test
  // time ran in 216s of wall clock on two workers with the machine ~0.6 of one core busy. So
  // worker count buys near-linear wall clock — 2 → 3m36s, 4 → 1m54s all green, 8 → 1m18s with
  // reading.spec's recency bloom failing. 4 is the ceiling that stays green, not "as many as there
  // are cores". Timings are from a 16-core machine; CI's 4 vCPUs gain less, and `retries: 1`
  // covers the odd contended flake.
  workers: 4,
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
