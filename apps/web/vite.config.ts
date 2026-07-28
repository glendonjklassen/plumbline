import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { READER_FONT_PATHS } from "./src/engine/fonts.generated";

// base "./" keeps the bundle host-agnostic: it works at a domain root (Azure
// SWA) and under a repo subpath (GitHub Pages) without a rebuild.
export default defineConfig({
  base: "./",
  define: {
    __BUILD_ID__: JSON.stringify(Date.now().toString(36)),
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
  ],
  build: {
    target: "es2022",
  },
});
