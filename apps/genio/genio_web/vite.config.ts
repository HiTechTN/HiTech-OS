import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

/**
 * Dev server proxies every daemon route, so the browser talks to one origin
 * (`:5173`) and SSE / loopback-only endpoints behave exactly as in production
 * where the built files are served by the daemon at `/ui/`.
 *
 * Change the target if the daemon runs elsewhere (or set VITE_GENIO_API when
 * the API is exposed behind a reverse proxy).
 */
const DAEMON = "http://127.0.0.1:8770";

const proxied = [
  "/health", "/telemetry", "/agent", "/caps", "/tools", "/approvals",
  "/policy", "/portal", "/audit", "/memory", "/mind",
];

export default defineConfig(({ command }) => ({
  // Built files are served by the daemon at /ui/ — dev stays at the root.
  base: command === "build" ? "/ui/" : "/",
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: Object.fromEntries(
      proxied.map((path) => [path, { target: DAEMON, changeOrigin: false }]),
    ),
  },
  build: {
    outDir: "dist",
    sourcemap: false,
    target: "es2022",
  },
}));
