import { defineConfig } from "@playwright/test";

/**
 * End-to-end tests against the running stack (docker compose up) or any other deployment:
 *   $env:BASE_URL='http://localhost:8080'; npx playwright test
 * Uses the installed Google Chrome, so no browser download is needed.
 */
export default defineConfig({
  testDir: "./e2e",
  outputDir: "./test-results",
  timeout: 45_000,
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"]],
  use: {
    baseURL: process.env.BASE_URL || "http://localhost:8080",
    channel: "chrome",
    trace: "retain-on-failure",
    acceptDownloads: true,
  },
});
