import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [react(), tailwind()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  build: {
    outDir: "../../crates/api/assets",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        manualChunks: {
          graph: ["@xyflow/react"],
          editor: [
            "@codemirror/state",
            "@codemirror/view",
            "@codemirror/lang-markdown",
            "@codemirror/commands",
            "@codemirror/language",
          ],
        },
      },
    },
  },
  server: {
    port: 5273,
    strictPort: false,
    proxy: {
      "/api": {
        target: process.env.HEIKAS_API_ORIGIN ?? "http://127.0.0.1:8731",
        changeOrigin: false,
      },
    },
  },
});
