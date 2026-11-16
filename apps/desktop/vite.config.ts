import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed port and the app to load from the bundled `dist/`.
export default defineConfig({
  root: "ui",
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: "localhost" },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "safari16",
    sourcemap: false,
    chunkSizeWarningLimit: 4096,
  },
  worker: { format: "es" },
  test: { root: ".", include: ["ui/src/**/*.test.ts"], environment: "node" },
});
