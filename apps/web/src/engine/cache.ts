// The one Cache API bucket the app uses, and the helper for putting things in
// it from the page or the engine worker.
//
// Why contexts cache their own downloads instead of leaving it to the service
// worker: on a FIRST visit the SW isn't controlling anything while the page
// loads, and it claims the clients somewhere in the middle of boot — so
// whether the ~12 MB data pack passed through its fetch handler came down to
// a race with clients.claim() (measured 2026-07-26: the wasm landed in the
// cache, the pack didn't, and the app couldn't boot offline afterwards).
// Downloading code already holds the bytes; stashing them costs nothing and
// is deterministic.

/** Must match the bucket name in public/sw.js — a plain script served from /,
 *  which cannot import this module. Change both together. */
export const CACHE = "plumbline-v1";

/** Delete what this build can no longer use.
 *
 *  Every versioned URL is content-addressed — pack files carry `?v=<pack
 *  hash>`, the wasm `?v=<build id>`, the JS/CSS their hashed filenames — so an
 *  update never overwrites an entry, it adds a new one beside it. Nothing ever
 *  removed the old: the bucket name is a constant, and the SW's activate only
 *  drops buckets under OTHER names. A reader who took three data updates was
 *  quietly carrying three whole ~12 MB packs (2026-07-27).
 *
 *  Conservative by construction: it deletes only what is positively identified
 *  as belonging to a version we are not running, so an interrupted update can
 *  never leave a device holding neither copy. Un-versioned entries (index.html,
 *  the fonts, the webmanifest) are never touched. Returns how many went. */
export async function pruneStale(keep: { versions: string[]; assets: Set<string> }): Promise<number> {
  if (typeof caches === "undefined") return 0;
  let gone = 0;
  try {
    const cache = await caches.open(CACHE);
    for (const req of await cache.keys()) {
      const url = new URL(req.url);
      const v = url.searchParams.get("v");
      const stale =
        v !== null
          ? !keep.versions.includes(v)
          : url.pathname.includes("/assets/") && !keep.assets.has(url.origin + url.pathname);
      if (stale && (await cache.delete(req))) gone++;
    }
  } catch {
    /* storage blocked: nothing to reclaim, and the app is unaffected */
  }
  return gone;
}

/** Store an already-fetched response under `url`. Pass a CLONE if the caller
 *  still needs the body. Best-effort: storage may be blocked or full, and a
 *  reader who can't cache should still be able to read. */
export async function stash(url: string, res: Response): Promise<void> {
  if (typeof caches === "undefined" || !res.ok) return;
  try {
    const cache = await caches.open(CACHE);
    // On a later visit the SW is controlling and has already stored this (in
    // fact it just served it) — rewriting the pack would mean ~12 MB of
    // pointless disk churn every launch. ignoreVary: our entries differ only
    // by the Origin header some requests carry (see public/sw.js).
    if (await cache.match(url, { ignoreVary: true })) return;
    await cache.put(url, res);
  } catch {
    /* private mode / quota: the app works, it just isn't offline yet */
  }
}
