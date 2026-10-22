// A stand-in engine worker, served from the app's own origin.
//
// The tests that need the engine to fail in a particular way replace its worker
// with a few lines of script. Those lines used to arrive as a blob: URL, and the
// production Content-Security-Policy (mirrored onto `vite preview` from
// ops/hetzner/cloud-init.yaml) allows workers from 'self' only. A blob worker is
// refused at construction, which raises the same `error` event a crash does — so
// every stub became "the engine stopped", and a test that accepts that message
// went on passing while testing a different failure from the one it names.
//
// So the stub's source rides in the query string of a same-origin URL, and a route
// answers it. The policy is the production one, unchanged.
//
// TWO CONDITIONS, both on the caller:
//   - `serveStubWorkers(page)` before the page constructs a stub.
//   - `test.use({ serviceWorkers: "block" })` around the test. sw.js answers every
//     same-origin GET itself (network-first), and a request the service worker
//     makes is invisible to `page.route` — CLAUDE.md's first trap. With the service
//     worker in the way the stub URL reaches `vite preview`, which 404s it, and the
//     worker fails to start: the very symptom this file exists to remove. None of
//     the stub tests is about the service worker, so blocking it costs them nothing.

import type { Page } from "@playwright/test";

/** The path the stubs are served at. Nothing in the app or the build uses it. */
export const STUB_WORKER_PATH = "/__e2e__/stub-worker.js";

/** Answer every stub-worker URL with the script in its `src` parameter. */
export async function serveStubWorkers(page: Page): Promise<void> {
  await page.route(`**${STUB_WORKER_PATH}?**`, (route) =>
    route.fulfill({
      status: 200,
      contentType: "text/javascript",
      body: new URL(route.request().url()).searchParams.get("src") ?? "",
    }),
  );
}

/** The page-side spelling of a stub URL, for code that runs in the browser
 *  (`addInitScript`, `page.evaluate` strings), where this module cannot be
 *  imported. Absolute from the origin's root, which is where preview serves. */
export const STUB_WORKER_URL_JS = `(src) => ${JSON.stringify(STUB_WORKER_PATH)} + "?src=" + encodeURIComponent(src)`;
