import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
      "@app": path.resolve(__dirname, "src/app"),
      "@core": path.resolve(__dirname, "src/core"),
      "@ds": path.resolve(__dirname, "src/design-system"),
      "@components": path.resolve(__dirname, "src/components"),
      "@modules": path.resolve(__dirname, "src/modules"),
      "@stores": path.resolve(__dirname, "src/stores"),
      "@domain": path.resolve(__dirname, "src/types"),
    },
  },
  clearScreen: false,
  // Porta fixa: o Tauri aponta o WebView para ela.
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: ["es2022", "chrome105", "safari13"],
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      output: {
        // Tudo que chama React.* ao carregar (lucide, zustand) fica no chunk do
        // React, senão pode executar antes dele. Konva e Leaflet só entram
        // quando um módulo os importa.
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (
            id.includes("/react/") ||
            id.includes("/react-dom/") ||
            id.includes("/react-router-dom/") ||
            id.includes("/react-router/") ||
            id.includes("/scheduler/") ||
            id.includes("/lucide-react/") ||
            id.includes("/zustand/")
          ) {
            return "vendor-react";
          }
          if (id.includes("/konva/") || id.includes("/react-konva/")) return "vendor-konva";
          if (id.includes("/leaflet/") || id.includes("/react-leaflet/")) return "vendor-leaflet";
          return undefined;
        },
      },
    },
    chunkSizeWarningLimit: 800,
  },
}));
