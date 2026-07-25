import { defineConfig } from "@playwright/test";

// E2E against the production build (vite preview). Locally, point at the
// system browser with CHROMIUM_BIN=/usr/bin/chromium; CI installs
// playwright's chromium. Each test gets a fresh storage state, so the app
// boots first-run (IndexedDB empty) unless a test seeds otherwise.
export default defineConfig({
  testDir: "./e2e",
  timeout: 120_000,
  retries: process.env.CI ? 1 : 0,
  workers: 1, // one preview server, engine boots are memory-heavy
  use: {
    baseURL: "http://localhost:4173",
    launchOptions: {
      executablePath: process.env.CHROMIUM_BIN || undefined,
    },
  },
  webServer: {
    command: "npm run preview",
    url: "http://localhost:4173",
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
