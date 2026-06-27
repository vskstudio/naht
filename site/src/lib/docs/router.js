import { readable } from 'svelte/store'

// Parse the current location hash into a route.
// '#/docs/<slug>' -> docs; '#/legal' -> legal; '#/', '', '#', or a bare
// '#anchor' -> landing.
export function parseHash(hash) {
  const h = hash ?? (typeof location !== 'undefined' ? location.hash : '')
  if (h === '#/docs' || h.startsWith('#/docs/')) {
    const rest = h.slice('#/docs/'.length)
    const [slug, anchor = ''] = rest.split('#')
    return { name: 'docs', slug: slug || 'introduction', anchor }
  }
  if (h === '#/legal' || h.startsWith('#/legal#')) {
    return { name: 'legal' }
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
