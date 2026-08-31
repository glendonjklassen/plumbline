import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { DEFAULT_FONT, FONT_ALL_FILES, FONT_FILES } from "./src/engine/fonts.generated";

// Two lists, because preloading and precaching answer different questions.
//
// PRELOAD is a boot-priority hint, so only the DEFAULT family belongs there:
// preloading every family would compete with the data pack for bandwidth on the
// one path where nothing else can proceed, to fetch type nobody has chosen.
//
// PRECACHE is the offline promise, so every family goes in (~1 MB, once): a
// reader who picks Fira Code and then gets on a plane must have Fira Code.
// Leaving the rest to be picked up incidentally by sw.js on first use would make
// "can I read offline" depend on whether that fetch happened while the SW was
// controlling.
const fontPathsOf = (token: string): string[] =>
  Object.values(FONT_FILES[token]).filter((p): p is string => typeof p === "string");
const DEFAULT_FONT_PATHS: string[] = fontPathsOf(DEFAULT_FONT);
// FONT_ALL_FILES, not FONT_FILES: the latter is the engine worker's measurement
// list and omits the chrome-only static bolds (Atkinson), which the offline
// precache still owes the document.
const ALL_FONT_PATHS: string[] = [...FONT_ALL_FILES];

// base "./" keeps the bundle host-agnostic: it works at a domain root and under
// a repo subpath (GitHub Pages) without a rebuild.

// One build id, shared by `define` (so the app knows what it is running) and the
// emitted shell manifest (so the precache knows what it is storing). Two
// independently-computed ids would disagree by a millisecond and the sweep would
// treat the running build as stale.
const BUILD_ID = Date.now().toString(36);

export default defineConfig({
  base: "./",
  define: {
    __BUILD_ID__: JSON.stringify(BUILD_ID),
    // The release tag, so a reader can tell us which build they are on. The
    // release workflow passes it; a local build honestly says "dev".
    __APP_VERSION__: JSON.stringify(process.env.PLUMBLINE_VERSION ?? "dev"),
  },
  plugins: [
    svelte(),
    // The reader faces are render-blocking and named by content hash, so they
    // cannot be hardcoded in index.html; discovered through fonts.css they are
    // two round trips into the boot. Preload them from the generated module.
    {
      name: "plumbline-preload-fonts",
      transformIndexHtml(html: string) {
        return {
          html,
          tags: DEFAULT_FONT_PATHS.map((href) => ({
            tag: "link",
            attrs: {
              rel: "preload",
              href: `./${href}`,
              as: "font",
              type: "font/woff2",
              crossorigin: "anonymous",
            },
            injectTo: "head-prepend" as const,
          })),
        };
      },
    },
    // The exact list of files that make up the app SHELL, so the offline
    // precache is driven by what the build produced rather than by what one page
    // happened to load. (Scraping `performance.getEntriesByType("resource")`
    // misses lazily-imported chunks, and the same list is the cache sweep's
    // keep-set, so it would also delete assets of a build this page had not
    // loaded — including one an update had just downloaded.)
    //
    // Deliberately NOT listed: sw.js (a service worker must never be served from
    // the cache it manages); the pack and wasm, which are the depot's business;
    // og-image.png, fetched by remote crawlers and never by the app; and
    // 404.html + CNAME, neither of which the app ever requests (CNAME is not an
    // HTTP resource, and once anything is stored sw.js answers an unfetchable
    // navigation with the shell itself). All still ship — Vite copies public/
    // verbatim; this list decides what goes in the DEPOT, not what goes in dist.
    {
      name: "plumbline-shell-manifest",
      generateBundle(_options, bundle) {
        const emitted = Object.keys(bundle).filter((n) => n !== "index.html");
        const publicFiles = [
          "fonts.css",
          "manifest.webmanifest",
          "icon.svg",
          // The three icons the webmanifest declares, and no others (30 KB). An
          // install icon is fetched by the BROWSER at install time, from a page
          // that may already be offline, so they belong in the depot even though
          // app code never reads them.
          "icon-192.png",
          "icon-512.png",
          "icon-maskable-512.png",
          // The iOS home-screen icon: same kind of file, wanted at the same
          // moment, 3 KB.
          "apple-touch-icon-180.png",
          ...ALL_FONT_PATHS,
        ];
        this.emitFile({
          type: "asset",
          // Unhashed on purpose: the app has to be able to ask for it by name.
          fileName: "shell-manifest.json",
          source: JSON.stringify(
            { buildId: BUILD_ID, files: ["index.html", ...emitted, ...publicFiles].sort() },
            null,
            2,
          ),
        });
      },
    },
  ],
  build: {
    target: "es2022",
  },
});
