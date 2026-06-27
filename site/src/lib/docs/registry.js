// Compile-time registry of every doc page. Each page is an mdsvex .svx file under
// src/content/<locale>/<slug>.svx — imported both as a Svelte component (for rendering)
// and as raw text (for the search index). Frontmatter is exposed as `metadata`.

const components = import.meta.glob('../../content/**/*.svx', { eager: true })
const sources = import.meta.glob('../../content/**/*.svx', { eager: true, query: '?raw', import: 'default' })

const KEY = /\/content\/([^/]+)\/([^/]+)\.svx$/

// registry[locale][slug] = { component, metadata, raw }
const registry = {}
for (const [path, mod] of Object.entries(components)) {
  const m = KEY.exec(path)
  if (!m) continue
  const [, locale, slug] = m
  ;(registry[locale] ??= {})[slug] = {
    component: mod.default,
    metadata: mod.metadata ?? {},
    raw: sources[path] ?? '',
  }
}

export const FALLBACK = 'en'

// Resolve a page for a locale, falling back to English when a translation is missing.
export function getDoc(locale, slug) {
  return registry[locale]?.[slug] ?? registry[FALLBACK]?.[slug] ?? null
}

// Metadata only (title/icon/accent/desc) — used by the sidebar before render.
export function getMeta(locale, slug) {
  return getDoc(locale, slug)?.metadata ?? {}
}

export function hasDoc(slug) {
  return Boolean(registry[FALLBACK]?.[slug])
}

function slugify(text) {
  return text.toLowerCase().trim().replace(/[^\w\s-]/g, '').replace(/\s+/g, '-')
}

// Flat search index for a locale: one entry per page + one per H2/H3 heading.
// Headings are parsed from the raw .svx (frontmatter + <script> blocks stripped).
export function buildSearchIndex(locale, navOrder) {
  const pages = registry[locale] ? { ...registry[FALLBACK], ...registry[locale] } : registry[FALLBACK] ?? {}
  const slugs = navOrder?.length ? navOrder.filter((s) => pages[s]) : Object.keys(pages)
  const idx = []
  for (const slug of slugs) {
    const entry = pages[slug]
    if (!entry) continue
    const title = entry.metadata?.title ?? slug
    idx.push({ slug, title, heading: null, id: null, text: title })
    const body = entry.raw
      .replace(/^---[\s\S]*?---/, '')
      .replace(/<script[\s\S]*?<\/script>/g, '')
    for (const m of body.matchAll(/^(#{2,3})\s+(.+)$/gm)) {
      const text = m[2].replace(/[*`_]/g, '').trim()
      idx.push({ slug, title, heading: text, id: slugify(text), text })
    }
  }
  return idx
}
