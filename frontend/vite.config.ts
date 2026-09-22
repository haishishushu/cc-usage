import path from "node:path"
import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import tailwindcss from "@tailwindcss/vite"

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  // Tauri 开发时固定端口，构建产物输出到 dist 供 Tauri 打包
  server: { port: 5173, strictPort: true },
  build: { outDir: "dist", emptyOutDir: true },
})
