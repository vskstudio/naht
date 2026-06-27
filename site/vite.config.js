import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// Relative base so the built `dist/` works when opened from any path (GitHub Pages subpath, file://, …).
export default defineConfig({
  base: './',
  plugins: [svelte()],
  test: {
    environment: 'jsdom',
    environmentOptions: { jsdom: { url: 'http://localhost/' } },
    setupFiles: ['./src/test-setup.js'],
    include: ['src/**/*.test.js'],
  },
})
