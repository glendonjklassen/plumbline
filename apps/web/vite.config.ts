import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { READER_FONT_PATHS } from "./src/engine/fonts.generated";

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
          tags: READER_FONT_PATHS.map((href) => ({
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
    {
      name: "plumbline-shell-manifest",
      generateBundle(_options, bundle) {
        const emitted = Object.keys(bundle).filter((n) => n !== "index.html");
        const publicFiles = [
          "fonts.css",
          "manifest.webmanifest",
          "icon.svg",
          "icon-128.png",
          "icon-256.png",
          // The home-screen icon, alongside the webmanifest's own: same kind of
          // file, wanted at the same moment, and 3 KB.
          "apple-touch-icon-180.png",
          ...READER_FONT_PATHS,
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
