import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  build: {
    // The local performance baseline follows this manifest rather than
    // guessing which hashed assets are required for the initial render.
    manifest: true,
    rollupOptions: {
      output: {
        // Lazy component names add no runtime value inside this device-local
        // package. Keep the full content hash for identity while omitting the
        // repeated names from the startup loader and manifest.
        chunkFileNames: "assets/[hash].js",
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
});
