import { defineConfig } from "@playwright/test";

// E2E smoke tests for the dashboard.
//
// First-time setup (per machine):  npx playwright install chromium
// Run:                             npm run test:e2e
//
// The config builds the app and starts it via `next start` on port 3100.
// The tests run without a backend, so they exercise the dashboard's
// FALLBACK-data path on purpose.
export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:3100",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "npm run build && npm run start",
    env: { PORT: "3100" },
    url: "http://localhost:3100",
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
});
