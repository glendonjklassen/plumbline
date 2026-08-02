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

import { DEPOT, depotAvailable, depotHas, requestPersistence } from "./depot";
import { assetUrl } from "./pack";

/** Exported for update.ts, which reads `buildId` off the same file to notice a
 *  deploy. The FILE is this module's business; the SHAPE is shared. */
export interface ShellManifest {
  buildId: string;
  files: string[];
}

/** The shell this build is made of, or null when the manifest cannot be read (a
 *  dev server, or offline before it was ever stored).
 *
 *  Module-private: `precacheShell` is the only caller and the only thing that
 *  should be. update.ts deliberately fetches this file itself with `no-store` —
 *  it must see the DEPLOY, not our stored copy — and reusing this cache-falling-
 *  back reader there would make every update check answer "no update". */
async function fetchShellManifest(): Promise<ShellManifest | null> {
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
  if (!depotAvailable()) return []; // no Cache API (private mode, http)
  try {
    const manifest = await fetchShellManifest();
    const base = new URL("./", location.href).href;
    const indexUrl = assetUrl("index.html");
    // The bare base as well as index.html: a navigation to "/" is a different
    // cache key from "/index.html", and both have to answer offline.
    const docs = [base, indexUrl];
    // Everything that is NOT the document. The document is stored last and
    // separately — see below, this ordering is the whole fix.
    const assets = new Set<string>([assetUrl("shell-manifest.json")]);
    for (const f of manifest?.files ?? []) if (assetUrl(f) !== indexUrl) assets.add(assetUrl(f));

    const cache = await caches.open(DEPOT);
    await Promise.all(
      [...assets].map(async (url) => {
        // Never re-download what is already stored. depotHas carries the
        // ignoreVary this lookup needs: these responses come back `Vary: Origin`,
        // and a copy stored for a request whose Origin header differs from this
        // one's would otherwise look absent — and get shadowed by a new entry.
        if (await depotHas(url)) return;
        await cache.add(url).catch(() => {
          /* checked below by presence, not by this error */
        });
      }),
    );

    // COMPLETE MEANS PRESENT, not "no fetch threw". `cache.add` is not the only
    // writer of these entries — the page's own `<script>` and `<link>` loads go
    // through the service worker's immutable path, which stores them too — so an
    // `add` can reject while the file is on disk anyway (a duplicate put, a race
    // with that load). Trusting the error channel made this report "incomplete"
    // for a shell that was entirely fine, which then skipped the promotion below
    // and left the stale document in place. Ask the cache instead.
    const missing: string[] = [];
    for (const url of assets) if (!(await depotHas(url))) missing.push(url);
    const complete = missing.length === 0;

    // ── THE DOCUMENT GOES LAST, AND ONLY IF THE SHELL IT NAMES IS HERE ────────
    //
    // A cached document is a PROMISE that the bundles it names can be served.
    // Breaking that promise is a white screen with no error and nothing to tap:
    // the document is served, `assets/index-<hash>.js` is not, and `#app` is
    // never mounted (reported 2026-07-31, on a plane).
    //
    // Two faults produced it, and both are fixed here. The document used to be a
    // peer of the assets in one unordered `Promise.all`, so it could land without
    // them. And it was guarded by `depotHas`, which meant that once stored it was
    // NEVER REPLACED — so the bare-base key kept the FIRST build's document
    // forever while `pruneToPin` went on keeping only the CURRENT build's assets
    // and deleting the ones that document asks for. An installed PWA opens
    // `start_url: "./"`, which is that exact key, so every reader who updated and
    // then opened the app offline was served a document whose bundle had been
    // reclaimed.
    //
    // So: re-fetch the document every time (it is a few KB), store it under BOTH
    // keys from ONE response so the two can never disagree with each other, and
    // do it only when every other shell file is confirmed present.
    // AN INCOMPLETE SHELL RECLAIMS NOTHING. The caller prunes with what this
    // returns, and `pruneToPin` refuses an empty list — so returning nothing is
    // how "do not reclaim yet" is said. Skipping only the promotion was not
    // enough, and left a state worse than the bug: the stale document stayed
    // while the prune went ahead and deleted the very bundle it names. Promotion
    // and reclamation are one decision, taken here.
    if (!complete || !manifest) return [];

    // `no-store` so this is the DEPLOYED document and not our own stored copy.
    // sw.js refuses to cache a no-store request, which is what keeps this the
    // only writer of these keys.
    const res = await fetch(indexUrl, { cache: "no-store" });
    if (!res.ok) return [];
    for (const key of docs) await cache.put(key, res.clone());

    // The shell is safe now, so ask the browser to keep it. The reader has used
    // the app at least once, which is the engagement signal persistence is
    // granted on. Nothing downstream assumes it was granted.
    void requestPersistence();

    return manifest.files;
  } catch {
    /* storage blocked or full: the app still runs, it just isn't offline yet */
    return [];
  }
}
