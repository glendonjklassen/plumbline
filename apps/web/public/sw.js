// Plumbline service worker: serves the app SHELL offline, and nothing else.
//
// WHAT IT DOES NOT TOUCH, and why that is the point. The data pack, the wasm
// engine and the pin are owned by app code (src/engine/depot.ts) and are read
// straight out of the same Cache bucket by the engine worker. They get an early
// `return` below, so this file is not in their request path at all.
//
// That is not tidiness — it removes a race. On a first visit the service worker
// is not controlling the page while the shell loads, and it claims clients
// somewhere in the middle of boot, so whether the ~10 MB pack passed through a
// fetch handler was a coin toss (measured 2026-07-26: the wasm landed in the
// cache, the pack did not, and the app could not boot offline afterwards). A
// dedicated worker also inherits its creator's controller at creation, so the
// engine worker spawned during an uncontrolled load is itself uncontrolled.
// Nothing the engine needs may depend on winning that race.
//
// What is left is the part only a service worker can do: answer a navigation
// when there is no network.
//
//   - Vite's content-hashed assets and the content-hashed fonts: cache-first.
//     They never change under one URL.
//   - Navigations and the unversioned public files (index.html, fonts.css,
//     manifest.webmanifest, icons): network-first with a 3.5 s timebox, then the
//     stored copy. `fonts.css` is render-blocking, and a navigation can stall on
//     a dozing radio, so the timebox still earns its keep here even though the
//     pack — what it was originally written for — has left.
//
// ONE bucket, shared with the depot, deliberately: `activate` deletes every
// bucket it does not recognise, so a second name is a bucket an older service
// worker can wipe. Separation is enforced by the early return, not by storage.
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

/** The one key the app shell is stored under. */
const shellKey = () => new URL("./index.html", location.href).href;

/** Whether a successful network response may be written to the cache.
 *
 *  Three refusals, all of which were white-screen bugs:
 *
 *  1. `cache: "no-store"` requests. The update check asks for a small manifest
 *     with no-store precisely because it wants the deployed truth rather than our
 *     copy; caching that answer defeats the request and, when it was index.html,
 *     stored a shell whose bundles did not exist yet.
 *  2. index.html fetched as DATA (not a navigation). Same shape: a newer document
 *     written into the cache while `/assets/*` for that build are absent, so the
 *     next offline launch is served a shell that asks for a bundle nobody has.
 *  3. THE DOCUMENT AT ALL, navigation included (2026-07-31). Refusals 1 and 2
 *     had the right reason and drew the line in the wrong place: a navigation
 *     writes the same broken pairing as a data fetch, it just takes a reader
 *     opening the app after a deploy rather than a stray fetch to do it. The
 *     newly-deployed document landed here immediately, while its bundles arrived
 *     only as the page happened to request them — so any interruption in that
 *     window left a document naming a bundle nobody had, and the next offline
 *     launch was blank.
 *
 *     `precache.ts` is now the ONLY writer of these two keys. It stores the
 *     document last, under both keys from one response, and only once every other
 *     shell file is confirmed present — which is the atomicity this file cannot
 *     provide, because it sees one request at a time and knows nothing about the
 *     rest of the build.
 *
 *     The cost is that a reader who closes the tab in the seconds before the
 *     precache runs has no stored document yet, and an offline launch then gets
 *     the browser's own offline page. That is a worse first visit and a far better
 *     failure: it says what happened, and it repairs itself on the next load. */
function mayCache(req, url) {
  if (req.cache === "no-store" || req.cache === "no-cache") return false;
  // By PATHNAME, not href (2026-07-30): compared as full URLs, any query string
  // walked straight past this — `index.html?x`, `/?x`, a cache-buster on an
  // update check — and the document was cached by a plain fetch after all.
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
  // being here would make this a SECOND writer of the same entries — and the
  // reconciler's guarantee is that a pin is only written once every file it names
  // is verified present, which a background writer could quietly undermine.
  // Also: `?h=` and `?v=` URLs are all depot URLs now, which is why the old
  // "anything versioned is cache-first" rule goes with them.
  if (url.pathname.includes("/pack/") || url.pathname.includes("/__depot/")) return;
  if (url.pathname.endsWith("plumbline_ffi.wasm")) return;

  // Content-hashed under a stable name: Vite's bundles, and the subsetted reader
  // faces (which carry their hash IN the filename, so this path rule is safe —
  // before they were hashed, a font replaced under the same name would have been
  // served from cache forever).
  const immutable = url.pathname.includes("/assets/") || url.pathname.includes("/fonts/");

  // `ignoreVary` on every lookup: these are our own same-origin files, keyed by
  // URL. Responses come back `Vary: Origin`, and Vite's <script crossorigin>
  // asset requests DO send Origin while a plain precache fetch does not —
  // honouring Vary made a cached entry invisible to the very request it was
  // stored for, and the app failed to boot offline with everything already on
  // disk (2026-07-26).
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
    // Network-first, TIMEBOXED (2026-07-26): a stalled connection must never hang
    // the app. It was written for the pack manifest, which used to pend forever
    // while the app sat on a loading screen; the manifest has since left both this
    // file and the boot path, but the classes that remain here can stall the same
    // way — a navigation, and render-blocking `fonts.css`. After 3.5 s the stored
    // copy is served; updates land on the next healthy load.
    event.respondWith(
      (async () => {
        try {
          const res = await Promise.race([
            fetch(req),
            new Promise((_, reject) => setTimeout(() => reject(new Error("sw-timeout")), 3500)),
          ]);
          if (res.ok && mayCache(req, url)) {
            // Clone SYNCHRONOUSLY, before the page consumes the body — a
            // deferred clone throws "already used" and silently left the
            // manifest uncached, so the offline fallback never had it.
            const copy = res.clone();
            // Stored under the request, full stop. There used to be a branch here
            // mapping navigations onto the canonical shell key — because storing
            // `req` let every distinct deep link (`/?at=Ps 23:1`, `/?church=…`)
            // accumulate its own copy of index.html, and offline one of those
            // stale copies would be served for that exact link, naming a bundle
            // that had since been pruned. That whole class is gone with refusal 3
            // in `mayCache`: the document never reaches this line, under any key,
            // so there is nothing left to canonicalise.
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
