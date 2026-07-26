// Plumbline service worker: the app works offline after the first visit.
// - Versioned resources (?v=… pack files, the build-stamped wasm) and Vite's
//   content-hashed assets are cache-first — they never change under one URL.
// - Everything else same-origin (index.html, manifest.json, fonts) is
//   network-first with cache fallback, so updates land when online and the
//   app still boots when not.
const CACHE = "plumbline-v1";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) =>
  e.waitUntil(
    (async () => {
      // Drop superseded buckets before claiming clients. Without this an
      // already-installed PWA keeps its old bucket forever — including the
      // pre-rename "pure-study-v1", which holds a whole stranded copy of the
      // pack (tens of MB) that nothing will ever read again.
      for (const key of await caches.keys()) if (key !== CACHE) await caches.delete(key);
      await self.clients.claim();
    })(),
  ),
);

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;
  const url = new URL(req.url);
  if (url.origin !== location.origin) return;

  const immutable =
    url.searchParams.has("v") || url.pathname.includes("/assets/") || url.pathname.includes("/fonts/");

  // `ignoreVary` on every lookup: these are our own same-origin files, keyed
  // by URL (pack files carry ?v=). Responses come back `Vary: Origin`, and
  // Vite's <script crossorigin> asset requests DO send Origin while a plain
  // precache fetch does not — honouring Vary made a cached entry invisible to
  // the very request it was stored for, and the app failed to boot offline
  // with everything already on disk (2026-07-26).
  const MATCH = { ignoreVary: true };

  if (immutable) {
    event.respondWith(
      caches.open(CACHE).then(async (cache) => {
        const hit = await cache.match(req, MATCH);
        if (hit) return hit;
        try {
          const res = await fetch(req);
          if (res.ok) {
            const copy = res.clone();
            cache.put(req, copy);
          }
          return res;
        } catch {
          // Offline with nothing cached: fail as a response, not as a
          // rejected respondWith (which surfaces as a cryptic ERR_FAILED).
          return Response.error();
        }
      }),
    );
  } else {
    // Network-first, TIMEBOXED (2026-07-26): a stalled mobile connection must
    // never hang boot — the manifest fetch used to pend forever and the app
    // sat on the "preparing your study tools" preview. After 3.5s the cached
    // copy is served; updates land on the next healthy load.
    event.respondWith(
      (async () => {
        try {
          const res = await Promise.race([
            fetch(req),
            new Promise((_, reject) => setTimeout(() => reject(new Error("sw-timeout")), 3500)),
          ]);
          if (res.ok) {
            // Clone SYNCHRONOUSLY, before the page consumes the body — a
            // deferred clone throws "already used" and silently left the
            // manifest uncached, so the offline fallback never had it.
            const copy = res.clone();
            caches.open(CACHE).then((cache) => cache.put(req, copy));
          }
          return res;
        } catch {
          const hit = await caches.match(req, MATCH);
          if (hit) return hit;
          // Only navigations fall back to the app shell — handing HTML to a
          // JSON fetch would turn one failure into a stranger one.
          if (req.mode !== "navigate") return Response.error();
          return (
            (await caches.match(new URL("./index.html", location.href).href, MATCH)) ??
            (await caches.match(new URL("./", location.href).href, MATCH)) ??
            Response.error()
          );
        }
      })(),
    );
  }
});
