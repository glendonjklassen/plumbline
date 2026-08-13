import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { DEFAULT_FONT, FONT_ALL_FILES, FONT_FILES } from "./src/engine/fonts.generated";

// Two different lists, because preloading and precaching answer two different
// questions.
//
// PRELOAD is a boot-priority hint: it says "fetch this now, ahead of what you
// would otherwise discover later". Only the DEFAULT family belongs there — a
// preload of four families would compete with the data pack for bandwidth on
// the one path where nothing else can proceed, to fetch type the reader has not
// chosen.
//
// PRECACHE is the offline promise, and the offline promise is a test, not a
// hope (CLAUDE.md). Every family goes in: a reader who picks Fira Code and then
// gets on a plane must have Fira Code. Leaving the non-default families to be
// picked up incidentally by sw.js on first use would make "can I read offline"
// depend on whether that fetch happened while the SW was controlling — exactly
// the kind of conditional the depot rules exist to remove. ~1 MB, once.
const fontPathsOf = (token: string): string[] =>
  Object.values(FONT_FILES[token]).filter((p): p is string => typeof p === "string");
const DEFAULT_FONT_PATHS: string[] = fontPathsOf(DEFAULT_FONT);
// FONT_ALL_FILES, not FONT_FILES: the latter is the engine worker's
// measurement list and deliberately omits the chrome-only static bolds
// (Atkinson), which the offline precache still owes the document.
const ALL_FONT_PATHS: string[] = [...FONT_ALL_FILES];

// base "./" keeps the bundle host-agnostic: it works at a domain root (Azure
// SWA) and under a repo subpath (GitHub Pages) without a rebuild.

// One build id, used by both `define` (so the app knows what it is running) and
// the emitted shell manifest (so the precache knows what it is storing). Two
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
    // cannot be hardcoded in index.html — and they were previously discovered
    // only after fonts.css had itself been fetched and parsed, two round trips
    // into the boot. Preload them from the generated module instead.
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
    // Emit the exact list of files that make up the app SHELL, so the offline
    // precache can be driven by what the build produced rather than by what one
    // page happened to load.
    //
    // It used to scrape `performance.getEntriesByType("resource")`. That is
    // clever and it is wrong in both directions: a chunk imported lazily (a
    // route the reader had not opened) never appears, so it is missing offline;
    // and the same list doubles as the cache sweep's keep-set, which meant the
    // sweep would happily delete assets belonging to a build this page had not
    // loaded — including the one an update had just downloaded.
    //
    // Deliberately NOT listed: sw.js (the service worker must never be served
    // from the cache it manages), the pack + wasm, which are the depot's business
    // and are far too big to belong in a shell list, and og-image.png — the link
    // card is fetched by remote crawlers over the network, never by the app, so
    // precaching it would spend first-visit bytes on a file no reader can ever
    // read offline. It still ships: Vite copies public/ verbatim, and this list
    // decides what goes in the DEPOT, not what goes in dist.
    //
    // Also deliberately absent, and for the same reason: 404.html and CNAME.
    // Neither is ever fetched BY the app. CNAME is not an HTTP resource at all —
    // it is a line of text GitHub Pages reads at deploy time to bind the custom
    // domain. And 404.html is served by the HOST for a path the app never
    // requests; once anything is stored, `sw.js` answers an unfetchable
    // navigation with the shell itself, so an installed reader can never reach
    // it. Storing it would cost first-visit bytes for a page that only exists
    // for a visitor who has stored nothing yet.
    {
      name: "plumbline-shell-manifest",
      generateBundle(_options, bundle) {
        const emitted = Object.keys(bundle).filter((n) => n !== "index.html");
        const publicFiles = [
          "fonts.css",
          "manifest.webmanifest",
          "icon.svg",
          // The three icons the webmanifest declares, and no others. An install
          // icon is fetched by the BROWSER at install time, from a page that may
          // already be offline, so these belong in the depot even though app code
          // never reads them: 192 and 512 are the pair Chrome wants (launcher +
          // splash), and the maskable 512 is what Android actually crops. 30 KB
          // for all three.
          //
          // icon-128.png and icon-256.png were here and are gone: no installer
          // asks for those sizes, the manifest no longer names them, and a
          // keep-set entry nothing declares is exactly the drift this list is
          // supposed to prevent. Nothing in the app or the markup references
          // them any more, so dropping them here only stops paying for 12 KB on
          // every device; the two files themselves are now dead weight in
          // public/ and can go.
          "icon-192.png",
          "icon-512.png",
          "icon-maskable-512.png",
          // The home-screen icon, alongside the webmanifest's own: same kind of
          // file, wanted at the same moment, and 3 KB.
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
