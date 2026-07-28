// Put the app SHELL in the depot after the first visit, and sweep what this
// build can no longer use.
//
// The service worker can't do the storing itself: on a first visit it isn't
// controlling the page while index.html, the bundles and the fonts load, so none
// of them pass through its fetch handler and nothing is kept. The reader who
// opens a shared link once and then boards a plane would find a dead app — the
// pack and the wasm are already in the depot (the engine worker puts them there),
// but the shell that runs them wouldn't be.
//
// WHAT the shell is comes from `shell-manifest.json`, which the build emits (see
// the plugin in vite.config.ts). It used to be scraped from
// `performance.getEntriesByType("resource")` — whatever this page happened to
// load. That is clever and it is wrong in both directions:
//
//   - a chunk imported lazily, for a screen the reader had not opened, never
//     appeared in the timeline and so was missing offline;
//   - the same list doubled as the cache sweep's keep-set, so the sweep would
//     delete assets belonging to any build this page had not loaded — including
//     the one a pending update had just downloaded.
//
// A build-emitted list is exact, complete, and identical on every page.

import { DEPOT, depotHas, requestPersistence } from "./depot";
import { assetUrl } from "./pack";

export interface ShellManifest {
  buildId: string;
  files: string[];
}

/** The shell this build is made of, or null when the manifest cannot be read (a
 *  dev server, or offline before it was ever stored). */
export async function fetchShellManifest(): Promise<ShellManifest | null> {
  const url = assetUrl("shell-manifest.json");
  try {
    const res = await fetch(url);
    if (res.ok) return (await res.json()) as ShellManifest;
  } catch {
    /* offline — fall through to the stored copy */
  }
  try {
    const hit = await caches.match(url, { ignoreVary: true });
    return hit ? ((await hit.json()) as ShellManifest) : null;
  } catch {
    return null;
  }
}

/** Store this build's shell. Returns the shell file list, so the caller can hand
 *  it to the worker's prune — which owns reclamation now, because the pin lives
 *  there and the pin is the authority on what to keep. Pruning happens only AFTER
 *  this resolves, never before: it must not be able to strand a half-updated app. */
export async function precacheShell(): Promise<string[]> {
  if (typeof caches === "undefined") return []; // no Cache API (private mode, http)
  try {
    const manifest = await fetchShellManifest();
    const base = new URL("./", location.href).href;
    // The bare base as well as index.html: a navigation to "/" is a different
    // cache key from "/index.html", and both have to answer offline.
    const urls = new Set<string>([base, assetUrl("shell-manifest.json")]);
    for (const f of manifest?.files ?? []) urls.add(assetUrl(f));

    const cache = await caches.open(DEPOT);
    await Promise.all(
      [...urls].map(async (url) => {
        // Never re-download what is already stored. depotHas carries the
        // ignoreVary this lookup needs: these responses come back `Vary: Origin`,
        // and a copy stored for a request whose Origin header differs from this
        // one's would otherwise look absent — and get shadowed by a new entry.
        if (await depotHas(url)) return;
        await cache.add(url).catch(() => {
          /* one missing asset must not sink the rest */
        });
      }),
    );

    // The shell is safe now, so ask the browser to keep it. The reader has used
    // the app at least once, which is the engagement signal persistence is
    // granted on. Nothing downstream assumes it was granted.
    void requestPersistence();

    return manifest?.files ?? [];
  } catch {
    /* storage blocked or full: the app still runs, it just isn't offline yet */
    return [];
  }
}
