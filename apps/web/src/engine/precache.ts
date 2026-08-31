// Put the app SHELL in the depot after the first visit, and sweep what this
// build can no longer use.
//
// The service worker cannot do the storing: on a first visit it is not controlling
// the page while index.html, the bundles and the fonts load, so none of them pass
// through its fetch handler. The reader who opens a shared link and then boards a
// plane would find a dead app — the pack and the wasm are in the depot already
// (the engine worker puts them there), but not the shell that runs them.
//
// WHAT the shell is comes from `shell-manifest.json`, emitted by the build (see the
// plugin in vite.config.ts), never scraped from
// `performance.getEntriesByType("resource")`: a lazily imported chunk for a screen
// the reader never opened would be missing offline, and the same list doubles as
// the sweep's keep-set, which would then delete assets belonging to any build this
// page had not loaded — including one a pending update had just downloaded.

import { DEPOT, depotAvailable, depotHas, requestPersistence } from "./depot";
import { assetUrl } from "./pack";

/** Exported for update.ts, which reads `buildId` off the same file to notice a
 *  deploy: the file is this module's business, the shape is shared. */
export interface ShellManifest {
  buildId: string;
  files: string[];
}

/** The shell this build is made of, or null when the manifest cannot be read (a dev
 *  server, or offline before it was ever stored).
 *
 *  Module-private: update.ts must fetch this file itself with `no-store`, since it
 *  needs the DEPLOY rather than our stored copy — reusing this cache-falling-back
 *  reader there would make every update check answer "no update". */
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

/** Store this build's shell. Returns the shell file list for the worker's prune,
 *  which owns reclamation because the pin (the authority on what to keep) lives
 *  there. Pruning happens only AFTER this resolves, never before, so it cannot
 *  strand a half-updated app. */
export async function precacheShell(): Promise<string[]> {
  if (!depotAvailable()) return []; // no Cache API (private mode, http)
  try {
    const manifest = await fetchShellManifest();
    const base = new URL("./", location.href).href;
    const indexUrl = assetUrl("index.html");
    // The bare base as well as index.html: a navigation to "/" is a different
    // cache key from "/index.html", and both have to answer offline.
    const docs = [base, indexUrl];
    // Everything that is NOT the document; the document is stored last and
    // separately (see below).
    const assets = new Set<string>([assetUrl("shell-manifest.json")]);
    for (const f of manifest?.files ?? []) if (assetUrl(f) !== indexUrl) assets.add(assetUrl(f));

    const cache = await caches.open(DEPOT);
    await Promise.all(
      [...assets].map(async (url) => {
        // Never re-download what is already stored. depotHas carries the ignoreVary
        // this lookup needs: these responses come back `Vary: Origin`, so a copy
        // stored for a request with a different Origin would otherwise look absent
        // and get shadowed by a new entry.
        if (await depotHas(url)) return;
        await cache.add(url).catch(() => {
          /* checked below by presence, not by this error */
        });
      }),
    );

    // COMPLETE MEANS PRESENT, not "no fetch threw". `cache.add` is not the only
    // writer of these entries — the page's own `<script>` and `<link>` loads go
    // through the service worker's immutable path — so an `add` can reject while
    // the file is on disk anyway. Ask the cache, not the error channel.
    const missing: string[] = [];
    for (const url of assets) if (!(await depotHas(url))) missing.push(url);
    const complete = missing.length === 0;

    // THE DOCUMENT GOES LAST, AND ONLY IF THE SHELL IT NAMES IS HERE. A stored
    // document is a promise that the bundles it names can be served; breaking it is
    // a white screen with nothing to tap. So: re-fetch it every time (a few KB, and
    // a document guarded by `depotHas` would never be replaced, leaving the first
    // build's document under the bare-base key an installed PWA opens as
    // `start_url`), store it under BOTH keys from ONE response so the two cannot
    // disagree, and only once every other shell file is confirmed present.
    //
    // An incomplete shell reclaims nothing: `pruneToPin` refuses an empty list, so
    // returning nothing is how "do not reclaim yet" is said. Promotion and
    // reclamation are one decision — skipping only the promotion would leave the
    // stale document in place while the prune deleted the bundle it names.
    if (!complete || !manifest) return [];

    // `no-store` so this is the DEPLOYED document and not our own stored copy.
    // sw.js refuses to cache a no-store request, which is what keeps this the
    // only writer of these keys.
    const res = await fetch(indexUrl, { cache: "no-store" });
    if (!res.ok) return [];
    for (const key of docs) await cache.put(key, res.clone());

    // The shell is safe now, so ask the browser to keep it — the reader has used the
    // app once, which is the engagement signal persistence is granted on. Nothing
    // downstream assumes it was granted.
    void requestPersistence();

    return manifest.files;
  } catch {
    /* storage blocked or full: the app still runs, it just isn't offline yet */
    return [];
  }
}
