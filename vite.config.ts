import { defineConfig } from "vite";
// @ts-expect-error Node types are not installed; Vite runs this file in Node.
import process from "node:process";

// Set by `tauri dev` when developing on a device (e.g. Android) instead of localhost.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(() => ({
  // Keep Rust compiler errors visible in the terminal.
  clearScreen: false,
  server: {
    // tauri.conf.json `devUrl` expects exactly this port.
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
