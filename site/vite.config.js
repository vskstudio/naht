import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// Relative base so the built `dist/` works when opened from any path (GitHub Pages subpath, file://, …).
export default defineConfig({
  base: './',
  plugins: [svelte()],
  server: { fs: { allow: ['..'] } },
})
