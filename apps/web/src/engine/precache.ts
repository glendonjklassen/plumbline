// Put the app SHELL in the service worker's cache after the first visit.
//
// The SW can't do this itself: on a first visit it isn't controlling the page
// while index.html, the JS/CSS bundles and the fonts load, so none of them
// pass through its fetch handler and nothing is stored. The reader who opens
// a shared link once, then boards a plane, would find a dead app — the pack
// and the wasm are already cached (the worker fetches those AFTER the SW
// claims the page), but the shell that runs them isn't.
//
// The page has the same Cache API, so it precaches its own shell from the
// resource timeline — whatever this build actually loaded, hashed names and
// all, with no asset list to keep in sync. The engine worker's downloads (the
// pack, the wasm) are stashed by the worker itself as they land.

import { CACHE, pruneStale } from "./cache";

/** The data pack is the engine worker's business, not the shell's. */
const skip = (url: string) => url.includes("/pack/");

/** @param keepVersions the `?v=` stamps still in use — this build's id and the
 *  pack version. Everything else versioned is swept once the shell is safely
 *  stored (never before: prune must not be able to strand a half-updated app). */
export async function precacheShell(keepVersions: string[] = []): Promise<void> {
  if (typeof caches === "undefined") return; // no Cache API (private mode, http)
  try {
    const base = new URL("./", location.href).href;
    const urls = new Set<string>([base, new URL("index.html", base).href]);
    for (const e of performance.getEntriesByType("resource")) {
      const url = e.name.split("?")[0];
      if (!url.startsWith(location.origin) || skip(url)) continue;
      urls.add(url);
    }
    const cache = await caches.open(CACHE);
    await Promise.all(
      [...urls].map(async (url) => {
        // Never re-download what the SW already stored on a later visit.
        // ignoreVary: these responses come back `Vary: Origin`, and the SW's
        // copy was stored for a request whose Origin header differs from this
        // one's — without it we'd re-add (and shadow) a perfectly good entry.
        if (await cache.match(url, { ignoreVary: true })) return;
        await cache.add(url).catch(() => {
          /* one missing asset must not sink the rest */
        });
      }),
    );
    if (keepVersions.length) await pruneStale({ versions: keepVersions, assets: urls });
  } catch {
    /* storage blocked or full: the app still runs, it just isn't offline yet */
  }
}
