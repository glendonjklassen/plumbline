// pure study service worker: the app works offline after the first visit.
// - Versioned resources (?v=… pack files, the build-stamped wasm) and Vite's
//   content-hashed assets are cache-first — they never change under one URL.
// - Everything else same-origin (index.html, manifest.json, fonts) is
//   network-first with cache fallback, so updates land when online and the
//   app still boots when not.
const CACHE = "pure-study-v1";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;
  const url = new URL(req.url);
  if (url.origin !== location.origin) return;

  const immutable =
    url.searchParams.has("v") || url.pathname.includes("/assets/") || url.pathname.includes("/fonts/");

  if (immutable) {
    event.respondWith(
      caches.open(CACHE).then(async (cache) => {
        const hit = await cache.match(req);
        if (hit) return hit;
        const res = await fetch(req);
        if (res.ok) cache.put(req, res.clone());
        return res;
      }),
    );
  } else {
    event.respondWith(
      fetch(req)
        .then((res) => {
          if (res.ok) caches.open(CACHE).then((cache) => cache.put(req, res.clone()));
          return res;
        })
        .catch(async () => (await caches.match(req)) ?? (await caches.match("./index.html")) ?? Response.error()),
    );
  }
});
