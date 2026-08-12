import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri 官方推荐的 Vite 配置：固定端口、忽略 src-tauri 目录变化，
// 详见 https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
