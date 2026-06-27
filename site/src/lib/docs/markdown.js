import { marked } from 'marked'
import { docs } from './nav.js'

function slugify(text) {
  return text.toLowerCase().trim().replace(/[^\w\s-]/g, '').replace(/\s+/g, '-')
}

// Turn GitHub-style callout blockquotes into <div class="callout KIND"> with rendered body.
function preprocessCallouts(md) {
  const re = /^> \[!(NOTE|WARNING|TIP|IMPORTANT)\][^\n]*\n((?:^>.*\n?)*)/gim
  return md.replace(re, (_m, kind, body) => {
    const inner = body.replace(/^> ?/gm, '').trim()
    return `\n<div class="callout ${kind.toLowerCase()}">\n\n${inner}\n\n</div>\n`
  })
}

// Render markdown -> { html, toc }. Heading ids, the TOC, and .md->#/docs link
// rewriting are done by post-processing the HTML with DOMParser (version-agnostic).
export function renderDoc(md) {
  const html0 = marked.parse(preprocessCallouts(md))
  const doc = new DOMParser().parseFromString(html0, 'text/html')
  const toc = []
  doc.querySelectorAll('h2, h3').forEach((h) => {
    const id = slugify(h.textContent)
    h.id = id
    toc.push({ id, text: h.textContent, level: h.tagName === 'H2' ? 2 : 3 })
  })
  doc.querySelectorAll('a[href]').forEach((a) => {
    const m = /^(?:\.\/)?([\w-]+)\.md(#.*)?$/.exec(a.getAttribute('href') || '')
    if (m) a.setAttribute('href', `#/docs/${m[1]}${m[2] || ''}`)
  })
  return { html: doc.body.innerHTML, toc }
}

// Flat index of doc titles + their headings, for the Ctrl+K search.
export function buildSearchIndex() {
  const idx = []
  for (const [slug, d] of Object.entries(docs)) {
    idx.push({ slug, title: d.title, heading: null, id: null, text: d.title })
    for (const h of renderDoc(d.raw).toc) {
      idx.push({ slug, title: d.title, heading: h.text, id: h.id, text: h.text })
    }
  }
  return idx
}
