import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// base "./" keeps the bundle host-agnostic: it works at a domain root (Azure
// SWA) and under a repo subpath (GitHub Pages) without a rebuild.
export default defineConfig({
  base: "./",
  plugins: [svelte()],
  build: {
    target: "es2022",
  },
});
