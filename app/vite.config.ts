import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    fs: { allow: [root] },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: { target: 'es2022', outDir: 'dist', emptyOutDir: true },
});
