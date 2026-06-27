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
  const seenIds = new Set()
  doc.querySelectorAll('h2, h3').forEach((h) => {
    let base = slugify(h.textContent)
    let id = base
    let n = 2
    while (seenIds.has(id)) { id = `${base}-${n++}` }
    seenIds.add(id)
    h.id = id
    toc.push({ id, text: h.textContent, level: h.tagName === 'H2' ? 2 : 3 })
  })
  const REPO_BLOB = 'https://github.com/vskstudio/naht/blob/main'
  const KNOWN = new Set(Object.keys(docs))
  doc.querySelectorAll('a[href]').forEach((a) => {
    const href = a.getAttribute('href') || ''
    const sib = /^(?:\.\/)?([\w-]+)\.md(#.*)?$/.exec(href)
    if (sib && KNOWN.has(sib[1])) {
      a.setAttribute('href', `#/docs/${sib[1]}${sib[2] || ''}`)
      return
    }
    if (/\.md(#.*)?$/.test(href)) {
      const clean = href.replace(/^(\.\/|\.\.\/)+/, '')
      a.setAttribute('href', `${REPO_BLOB}/${clean}`)
      a.setAttribute('target', '_blank')
      a.setAttribute('rel', 'noreferrer')
    }
  })
  // Wrap tables so they scroll horizontally on narrow viewports without breaking layout
  doc.querySelectorAll('table').forEach((table) => {
    const wrap = doc.createElement('div')
    wrap.className = 'table-wrap'
    table.parentNode.insertBefore(wrap, table)
    wrap.appendChild(table)
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
