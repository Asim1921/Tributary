import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  root: import.meta.dirname,
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022' },
  test: { include: ['src/**/*.test.ts', '../extension/**/*.test.js'] },
});
