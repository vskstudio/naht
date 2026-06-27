import { readable } from 'svelte/store'

// Parse the current location hash into a route.
// '#/docs/<slug>' -> docs; '#/', '', '#', or a bare '#anchor' -> landing.
export function parseHash(hash) {
  const h = hash ?? (typeof location !== 'undefined' ? location.hash : '')
  if (h === '#/docs' || h.startsWith('#/docs/')) {
    const slug = h.slice('#/docs/'.length).split('#')[0]
    return { name: 'docs', slug: slug || 'quickstart' }
  }
  return { name: 'landing' }
}

export const route = readable(parseHash(), (set) => {
  const update = () => set(parseHash())
  window.addEventListener('hashchange', update)
  return () => window.removeEventListener('hashchange', update)
})

export function navigate(to) {
  if (location.hash === to) return
  location.hash = to
}
