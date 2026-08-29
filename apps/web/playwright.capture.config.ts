import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  testMatch: /.*\.capture\.ts/,
  globalSetup: "./tests/global-setup.ts",
  globalTeardown: "./tests/global-teardown.ts",
  timeout: 300_000,
  expect: { timeout: 30_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    viewport: { width: 1440, height: 900 },
    colorScheme: "dark",
    locale: "en-GB",
    timezoneId: "Europe/London",
    video: { mode: "on", size: { width: 1440, height: 900 } },
  },
  projects: [
    {
      name: "capture",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 900 } },
    },
  ],
});
