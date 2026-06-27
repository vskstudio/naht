import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// Relative base so the built `dist/` works when opened from any path (GitHub Pages subpath, file://, …).
export default defineConfig({
  base: './',
  // No source maps in the production bundle — they would embed local filesystem
  // paths and original source, neither of which should ship to naht.dev.
  build: { sourcemap: false },
  plugins: [svelte({ extensions: ['.svelte', '.md', '.svx'] })],
  test: {
    environment: 'jsdom',
    environmentOptions: { jsdom: { url: 'http://localhost/' } },
    setupFiles: ['./src/test-setup.js'],
    include: ['src/**/*.test.js'],
  },
})
