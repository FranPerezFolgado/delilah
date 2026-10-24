import { defineConfig } from "vite";
import { configDefaults } from "vitest/config";
import react from "@vitejs/plugin-react";

// Port 1420 is the dev origin the daemon allows through CORS.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // e2e/ holds Playwright specs, run by `npm run e2e`, not by vitest.
  test: { exclude: [...configDefaults.exclude, "e2e/**"] },
  // A local desktop app: one bundle is fine.
  build: { chunkSizeWarningLimit: 1500 },
});
