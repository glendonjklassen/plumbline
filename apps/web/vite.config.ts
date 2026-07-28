import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

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
  plugins: [svelte()],
  build: {
    target: "es2022",
  },
});
