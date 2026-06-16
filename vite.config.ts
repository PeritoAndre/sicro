import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import fs from "node:fs";

const host = process.env.TAURI_DEV_HOST;

/**
 * pdf.js v6 decodifica imagens JPEG2000 (OpenJPEG) e JBIG2 via WASM. Copia os
 * binários de `pdfjs-dist/wasm` para `public/pdfjs-wasm` (servido em
 * `/pdfjs-wasm` no dev e empacotado pelo Tauri no build) — 100% offline e
 * sempre em sincronia com a versão instalada. Sem isso, páginas de PDFs
 * escaneados (JPX/JBIG2) ficam em branco no visualizador da Documentoscopia.
 */
function copyPdfjsWasm() {
  return {
    name: "sicro:copy-pdfjs-wasm",
    configResolved() {
      const src = path.resolve(__dirname, "node_modules/pdfjs-dist/wasm");
      const dst = path.resolve(__dirname, "public/pdfjs-wasm");
      try {
        fs.mkdirSync(dst, { recursive: true });
        fs.cpSync(src, dst, { recursive: true });
      } catch (e) {
        console.warn("[sicro] falha ao copiar wasm do pdf.js:", e);
      }
    },
  };
}

// Tauri expects a fixed port and disables HMR overlay during build.
export default defineConfig(async () => ({
  plugins: [react(), copyPdfjsWasm()],
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
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: ["es2022", "chrome105", "safari13"],
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // F12.9 — Bundle splitting. Divide o bundle inicial (que estava em
    // ~1.7 MB) em chunks lógicos:
    //   - vendor-react: React + ReactDOM + router + TODA lib React-aware
    //     (lucide-react, zustand) — coabitam pra garantir que React
    //     esteja carregado quando elas executam `React.forwardRef`.
    //     Esse é o ponto-chave: chunks que dependem de React PRECISAM
    //     compartilhar o mesmo bundle, senão a ordem de carga em fresh
    //     install (sem cache do WebView2) pode falhar com
    //     "Cannot read properties of undefined (reading 'forwardRef')".
    //   - vendor-tiptap: TipTap + ProseMirror + extensions (~360 KB)
    //   - vendor-konva: Konva + react-konva (~280 KB) — só usado em
    //     croqui/imagem; carregado on-demand pelo dynamic import. Como
    //     o lazy import garante que o vendor-react já carregou, fica OK.
    //   - vendor-leaflet: Leaflet + react-leaflet (~180 KB) — idem (lazy).
    //   - vendor-qrcode: lib QR (~30 KB).
    //   - vendor-misc: deps puramente sem React (polygon-clipping).
    rollupOptions: {
      output: {
        manualChunks(id) {
          // F12.9 — Pacotes só são identificados por substring do path
          // de node_modules. @tiptap/pm tem subpath exports (não pode
          // ser citado como entry plano), então casamos pelo prefixo.
          if (!id.includes("node_modules")) return undefined;
          if (
            id.includes("/react/") ||
            id.includes("/react-dom/") ||
            id.includes("/react-router-dom/") ||
            id.includes("/react-router/") ||
            id.includes("/scheduler/") ||
            // BUGFIX beta — Toda lib que importa React.* no topo do
            // módulo (forwardRef, createContext, hooks) precisa estar
            // no chunk do React. Senão o chunk dela pode ser executado
            // antes do vendor-react ter rodado a inicialização.
            id.includes("/lucide-react/") ||
            id.includes("/zustand/")
          ) {
            return "vendor-react";
          }
          if (id.includes("/@tiptap/") || id.includes("/prosemirror-")) {
            return "vendor-tiptap";
          }
          if (id.includes("/konva/") || id.includes("/react-konva/")) {
            return "vendor-konva";
          }
          if (id.includes("/leaflet/") || id.includes("/react-leaflet/")) {
            return "vendor-leaflet";
          }
          // Fórmula matemática (KaTeX + html-to-image) — carregado sob demanda
          // só quando se vê/edita/insere uma fórmula. Chunk próprio pra não
          // pesar no bundle base do editor.
          if (id.includes("/katex/") || id.includes("/html-to-image/")) {
            return "vendor-math";
          }
          return undefined; // deixa o Rollup decidir o resto
        },
      },
    },
    // Aumenta o threshold do warning ALÉM do que conseguimos otimizar,
    // mas mantemos visibilidade — qualquer chunk acima de 800 KB deve
    // ser investigado.
    chunkSizeWarningLimit: 800,
  },
}));
