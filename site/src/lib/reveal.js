// A Svelte action: fade + lift an element into view the first time it crosses the viewport.
// Honors prefers-reduced-motion (the CSS already short-circuits the transition) and degrades to
// "always visible" when IntersectionObserver is unavailable.
export function reveal(node, { delay = 0 } = {}) {
  node.style.setProperty('--delay', `${delay}ms`)

  if (typeof IntersectionObserver === 'undefined') {
    node.classList.add('in')
    return {}
  }

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          node.classList.add('in')
          observer.unobserve(node)
        }
      }
    },
    { threshold: 0.18, rootMargin: '0px 0px -8% 0px' },
  )

  observer.observe(node)
  return {
    destroy() {
      observer.disconnect()
    },
  }
}
