import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Port 1420 is the dev origin the daemon allows through CORS.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // A local desktop app: one bundle is fine.
  build: { chunkSizeWarningLimit: 1500 },
});
