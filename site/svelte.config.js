import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'
import { mdsvex } from 'mdsvex'
import rehypeSlug from 'rehype-slug'

/** mdsvex: author docs as .svx (Svelte components inside markdown). rehype-slug
 *  gives every heading a stable id so the right-rail TOC can link to it. */
const mdsvexConfig = {
  extensions: ['.md', '.svx'],
  rehypePlugins: [rehypeSlug],
}

export default {
  extensions: ['.svelte', '.md', '.svx'],
  preprocess: [vitePreprocess(), mdsvex(mdsvexConfig)],
}
