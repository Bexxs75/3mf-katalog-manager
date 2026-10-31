import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import type { Plugin } from "vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const web = process.env.VITE_TARGET === 'web';
const host = web ? undefined : process.env.TAURI_DEV_HOST;

const nativeModuleCheck: Plugin = {
  name: 'web-native-module-check',
  // Native enum initialization is irrelevant after build-time branches remove
  // every native consumer. Desktop module semantics remain untouched.
  transform(code, id) {
    if (id.includes('/@tauri-apps/')) return { code, map: null, moduleSideEffects: false };
  },
  generateBundle(_options, bundle) {
    const modules = Object.values(bundle).flatMap(chunk => chunk.type === 'chunk'
      ? Object.entries(chunk.modules).filter(([, info]) => info.renderedLength > 0).map(([id]) => id) : []);
    const native = modules.filter(id => id.includes('/@tauri-apps/'));
    if (native.length) this.error(`Native modules in web build: ${native.join(', ')}`);
    console.info(`Web bundle checked: ${modules.length} rendered modules, no @tauri-apps modules.`);
  },
};

export default defineConfig(() => ({
  plugins: [react(), ...(web ? [nativeModuleCheck] : [])],
  clearScreen: false,
  ...(web ? {
    base: '/',
    build: { outDir: 'dist-web' },
  } : {}),
  server: {
    // Preserve the browser Host (including port), so Origin validation still
    // compares the same authority. Server: KATALOG_ALLOWED_HOSTS=localhost:1420,127.0.0.1:1420
    ...(web ? { proxy: { '/api': { target: 'http://127.0.0.1:8080', changeOrigin: false } } } : {}),
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    globals: true,
  },
}));
