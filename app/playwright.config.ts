import { defineConfig, devices } from "@playwright/test";

// The daemon and Vite are started by e2e/setup.ts; baseURL comes from the env file it writes
// (see the `baseURL` fixture in e2e/fixtures.ts), since the port is only known at run time.
export default defineConfig({
  testDir: "e2e",
  workers: 1,
  fullyParallel: false,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  globalSetup: "./e2e/setup.ts",
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    ...devices["Desktop Chrome"],
    headless: true,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
});
