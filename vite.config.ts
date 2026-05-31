import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// Single source of truth for the version label rendered in the status bar.
// release-please manages version.txt (canonical) + .release-please-manifest.json;
// package.json / Cargo.toml / tauri.conf.json stay at 0.0.1 by project policy.
// Read at build time so the bundled JS always carries the current release.
// @ts-expect-error __dirname is a nodejs global
const APP_VERSION = readFileSync(resolve(__dirname, "version.txt"), "utf8").trim();

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(APP_VERSION),
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
