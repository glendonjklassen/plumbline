// Plumbline service worker: serves the app SHELL offline, and nothing else.
//
// The data pack, the wasm engine and the pin are owned by app code
// (src/engine/depot.ts) and read straight out of the same Cache bucket by the
// engine worker. They get an early `return` below, so this file is not in their
// request path — which removes a race rather than merely tidying: on a first
// visit the SW is not controlling the page while the shell loads and claims
// clients mid-boot, so whether a given file passed through a fetch handler was a
// coin toss. (A dedicated worker inherits its creator's controller at creation,
// so an engine worker spawned during an uncontrolled load is uncontrolled too.)
//
// What is left is the part only a service worker can do: answer a navigation
// when there is no network.
//
//   - Vite's content-hashed assets and the content-hashed fonts: cache-first.
//     They never change under one URL.
//   - Navigations and the unversioned public files (index.html, fonts.css,
//     manifest.webmanifest, icons): network-first with a 3.5 s timebox, then the
//     stored copy — `fonts.css` is render-blocking and a navigation can stall on
//     a dozing radio.
//
// ONE bucket, shared with the depot: `activate` deletes every bucket it does not
// recognise, so a second name is a bucket an older service worker can wipe.
// Separation is enforced by the early return, not by storage.
const CACHE = "plumbline-v1";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) =>
  e.waitUntil(
    (async () => {
      // Drop superseded buckets before claiming clients: an already-installed
      // PWA otherwise keeps a stranded copy of the pack (tens of MB) under its
      // old bucket name forever.
      for (const key of await caches.keys()) if (key !== CACHE) await caches.delete(key);
      await self.clients.claim();
    })(),
  ),
);

/** The one key the app shell is stored under. */
const shellKey = () => new URL("./index.html", location.href).href;

/** Whether a successful network response may be written to the cache. Two
 *  refusals, both of which were white-screen bugs:
 *
 *  1. `cache: "no-store"` / `"no-cache"` requests. The update check asks for a
 *     manifest that way precisely because it wants the deployed truth rather
 *     than our copy.
 *  2. THE DOCUMENT AT ALL, navigation included. A document cached here lands
 *     immediately while its `/assets/*` arrive only as the page requests them,
 *     so any interruption leaves a shell naming a bundle nobody has and the next
 *     offline launch is blank. `precache.ts` is the only writer of these keys:
 *     it stores the document last, under both keys from one response, and only
 *     once every other shell file is confirmed present — atomicity this file
 *     cannot provide, seeing one request at a time.
 *
 *  The cost is that a reader who closes the tab before the precache runs has no
 *  stored document, and an offline launch gets the browser's offline page: a
 *  worse first visit and a better failure, because it repairs itself. */
function mayCache(req, url) {
  if (req.cache === "no-store" || req.cache === "no-cache") return false;
  // By PATHNAME, not href: compared as full URLs, any query string walks
  // straight past this (`index.html?x`, `/?x`, a cache-buster on an update
  // check) and the document gets cached by a plain fetch after all.
  const shellPaths = [new URL("./index.html", location.href).pathname, new URL("./", location.href).pathname];
  if (shellPaths.includes(url.pathname)) return false;
  return true;
}

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;
  const url = new URL(req.url);
  if (url.origin !== location.origin) return;

  // THE DEPOT'S TERRITORY. App code reads these from the Cache API itself, so
  // handling them here would make this a second writer of the same entries — and
  // the reconciler's guarantee is that a pin is written only once every file it
  // names is verified present, which a background writer could undermine.
  if (url.pathname.includes("/pack/") || url.pathname.includes("/__depot/")) return;
  if (url.pathname.endsWith("plumbline_ffi.wasm")) return;

  // Cache-first BY PATH, which is safe only because both carry their hash in the
  // filename: Vite's bundles, and the subsetted reader faces.
  const immutable = url.pathname.includes("/assets/") || url.pathname.includes("/fonts/");

  // `ignoreVary` on every lookup: these are our own same-origin files, keyed by
  // URL. Responses come back `Vary: Origin` and Vite's <script crossorigin>
  // requests DO send Origin while a plain precache fetch does not, so honouring
  // Vary hides an entry from the very request it was stored for and the app
  // fails to boot offline with everything already on disk.
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
    // Network-first, TIMEBOXED: a stalled connection must never hang the app.
    // A navigation and render-blocking `fonts.css` both can. After 3.5 s the
    // stored copy is served; updates land on the next healthy load.
    event.respondWith(
      (async () => {
        try {
          const res = await Promise.race([
            fetch(req),
            new Promise((_, reject) => setTimeout(() => reject(new Error("sw-timeout")), 3500)),
          ]);
          if (res.ok && mayCache(req, url)) {
            // Clone SYNCHRONOUSLY, before the page consumes the body: a deferred
            // clone throws "already used" and silently leaves the entry uncached.
            const copy = res.clone();
            // Stored under the request, full stop — nothing to canonicalise,
            // because `mayCache` refusal 2 keeps the document off this line
            // under any key.
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
