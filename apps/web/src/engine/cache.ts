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
