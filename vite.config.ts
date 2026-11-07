import { defineConfig } from "vite";
import solid from "@solidjs/vite-plugin";

const host = process.env.TAURI_DEV_HOST;

// The Studio is the only webview and `index.html` is the only entry (ADR-013).
// Visualizer windows are native and never load this bundle.
export default defineConfig({
  plugins: [...solid()],
  clearScreen: false,
  server: {
    // Must match `build.devUrl` in src-tauri/tauri.conf.json and
    // `preflight::DEV_PORT`; strict so a busy port fails instead of moving.
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**", "**/references/**"] },
  },
  build: {
    target: "es2022",
  },
});
