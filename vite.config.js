import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

// Workaround for vite-plugin-svelte incorrectly parsing script blocks as CSS
const fixSvelteStylePlugin = {
  name: 'fix-svelte-style',
  enforce: 'pre',
  load(id) {
    // Return empty CSS for incorrectly identified CSS modules from bits-ui
    if (id.includes('node_modules/bits-ui') && id.includes('?svelte&type=style&lang.css')) {
      return '';
    }
  },
};

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [fixSvelteStylePlugin, sveltekit()],
  define: { global: "window" },

  build: {
    sourcemap: true,
  },

  optimizeDeps: {
    exclude: ["bits-ui"],
  },

  ssr: {
    noExternal: ["bits-ui"],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
