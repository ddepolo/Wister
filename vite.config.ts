import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri espera el servidor de desarrollo en un puerto fijo (ver src-tauri/tauri.conf.json).
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
  build: {
    // Una página por ventana: la de configuración y el overlay.
    rollupOptions: {
      input: { main: "index.html", overlay: "overlay.html" },
    },
  },
});
