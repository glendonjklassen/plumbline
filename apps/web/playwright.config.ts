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
// suite twice: the run is serialised (workers: 1) and every one of these tests
// boots the engine and downloads the pack, so a second full pass would cost
// minutes to re-assert things that have nothing to do with the engine.

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
 *    - the two stalled-origin boots in network.spec.ts — the document served
 *      from cache with no usable network. They stand in for the offline test
 *      below, because their "offline" is a real held-open socket underneath the
 *      browser rather than the browser's own emulation.
 *
 *  Two are missing on purpose, and both are findings rather than tidying:
 *
 *    - "boots offline after ONE visit" CANNOT run on WebKit. Playwright's
 *      context.setOffline(true) stops WebKit consulting the service worker at
 *      all: the reload dies with "WebKit encountered an internal error" and a
 *      page fetch throws TypeError. That is the harness, not us — a minimal
 *      cache-first service worker on a throwaway origin fails identically,
 *      while Chromium serves it from cache. The same WebKit device booted to
 *      John 3 in 222 ms with its origin genuinely refusing connections, which
 *      is why the stalled-origin pair above is the honest substitute.
 *    - "checking for an update cannot poison the cached shell" FAILS on WebKit,
 *      and the assertion is the one telling the truth: sw.js's mayCache()
 *      recognises "index.html asked for as data" by comparing url.href, so any
 *      query string walks straight past it and the document IS cached. Chromium
 *      caches it too — cache.keys() lists the entry — and passes only because
 *      the page reads the cache before the service worker's un-awaited put has
 *      landed. It belongs in this list once sw.js compares pathnames and the
 *      test stops racing the write; until then leaving it out is not weakening
 *      it, because it still runs (and still passes for that reason) on chromium.
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
  workers: 1, // one preview server, engine boots are memory-heavy
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
