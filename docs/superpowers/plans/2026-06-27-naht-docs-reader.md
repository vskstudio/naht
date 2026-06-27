# Naht Docs Reader Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `site/` with a sober, Satvrn-style 3-column documentation reader at `#/docs/*` that renders the repo's `docs/*.md` (single source of truth), with a collapsible sidebar, scrollspy "On this page" TOC, and a Ctrl+K client search — reusing the naht palette in a restrained register.

**Architecture:** Hash-based client routing turns `App.svelte` into a switch: `#/` renders the existing landing (extracted into `Landing.svelte`), `#/docs/<slug>` renders `DocsLayout.svelte`. Docs are imported raw (`?raw`) and rendered with `marked`; heading ids, the TOC, and `.md→#/docs` link rewriting are done by post-processing the rendered HTML with `DOMParser` (version-agnostic, runs client-side in the SPA).

**Tech Stack:** Svelte 5, Vite 5, `marked` (md→html, the only new runtime dep), plain CSS reusing `app.css` tokens.

## Global Constraints

- Stack stays **Svelte 5 + Vite**. The ONLY new runtime dependency is **`marked`**. No router lib, no search lib, no highlighter.
- `site/vite.config.js` keeps `base: './'` (static hosting). Add `server.fs.allow: ['..']` so the dev server can read `../docs/*.md`.
- The repo's `docs/*.md` are the **single source of truth** — render them, do not copy/reword their content.
- Reuse the existing `site/src/app.css` tokens. **Sober register:** neutral surfaces; accent (amber primary; periwinkle `--merge` / cyan `--studio` for links + active states) used sparingly; NO gradient hero in the docs view.
- Hash routing only treats `#/`-prefixed hashes as routes; bare in-page anchors on the landing (`#why`, `#sync`, …) must keep working.
- All motion gated behind `@media (prefers-reduced-motion: reduce)`.
- **Definition of done for every task:** `site/` `npm run build` is clean (zero warnings) AND the task's screenshot/interaction check passes.

## Verification model (read first)

Like the existing site, there is no unit-test harness; the logic here is small and first-party. The per-task gate is **zero-warning build + a Chrome screenshot/interaction check**.

- Dev server (run once, keep running): from `site/`, `npm run dev` (http://localhost:5173, HMR). On Windows run it backgrounded so it survives across tasks. If `:5173` is unreachable, start it.
- Screenshot helper (Windows + headless Chrome; Chrome can't write MSYS paths — write to a native `C:\…\Temp` path, then view it):

  ```bash
  "/c/Program Files/Google/Chrome/Application/chrome.exe" \
    --headless=new --disable-gpu --no-sandbox --hide-scrollbars \
    --virtual-time-budget=4000 --window-size=1440,1024 \
    --screenshot="C:\\Users\\Kara\\AppData\\Local\\Temp\\d.png" \
    "http://localhost:5173/#/docs/quickstart"
  ```

  Navigate to specific routes by changing the URL hash. For mobile use `--window-size=390,1600`. Headless Chrome cannot press Ctrl+K, so verify the search modal by temporarily forcing it open (see Task 5) or by reading the component's logic — note this in the report.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `site/package.json` | Modify | Add `marked` dependency |
| `site/vite.config.js` | Modify | `server.fs.allow: ['..']` for `../docs` raw imports |
| `site/src/lib/docs/router.js` | Create | Hash route parsing + `route` store + `navigate()` |
| `site/src/lib/docs/nav.js` | Create | `?raw` doc imports, `docs` map, `sections` sidebar config |
| `site/src/lib/docs/markdown.js` | Create | `renderDoc(md) → {html, toc}`; callouts; heading ids; link rewrite; `buildSearchIndex()` |
| `site/src/lib/docs/MarkdownView.svelte` | Create | Render a doc's HTML, expose its TOC |
| `site/src/lib/docs/DocsLayout.svelte` | Create | 3-column shell (TopBar · Sidebar · content · TocRight) |
| `site/src/lib/docs/TopBar.svelte` | Create | Logo, nav, search button |
| `site/src/lib/docs/Sidebar.svelte` | Create | Collapsible section nav, active state |
| `site/src/lib/docs/TocRight.svelte` | Create | "On this page" + scrollspy |
| `site/src/lib/docs/Search.svelte` | Create | Ctrl+K modal over the heading index |
| `site/src/lib/Landing.svelte` | Create | The current landing, moved out of `App.svelte` |
| `site/src/App.svelte` | Modify | Thin router switch: Landing vs DocsLayout |
| `site/src/app.css` | Modify | A few docs-content utility styles (callouts, prose) if needed |

---

## Task 1: Routing + landing extraction + marked dependency

**Files:**
- Modify: `site/package.json` (add `marked`)
- Modify: `site/vite.config.js` (`server.fs.allow`)
- Create: `site/src/lib/docs/router.js`
- Create: `site/src/lib/Landing.svelte`
- Modify: `site/src/App.svelte` (router switch + temporary DocsLayout stub)

**Interfaces:**
- Produces: `router.js` exports `route` (a Svelte `readable` store of `{name:'landing'} | {name:'docs', slug:string}`), `navigate(to:string)`, and `parseHash(hash?:string)`.
- Produces: `Landing.svelte` (no props) — the existing landing markup.

- [ ] **Step 1: Add `marked` and the fs.allow config**

```bash
cd site && npm install marked
```

In `site/vite.config.js`, add a `server.fs.allow` so dev can read `../docs`. Example final file:

```js
import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  base: './',
  plugins: [svelte()],
  server: { fs: { allow: ['..'] } },
})
```

(Preserve any existing options already in the file; only add the `base`/`server` keys if missing.)

- [ ] **Step 2: Create `router.js`**

```js
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
```

- [ ] **Step 3: Extract the landing into `Landing.svelte`**

Move the ENTIRE current contents of `site/src/App.svelte` (its `<script>`, all the markup — nav, `<Hero/>`, `<main>` sections, footer — and its `<style>`) verbatim into a new `site/src/lib/Landing.svelte`. Fix the relative import paths now that the file moved one directory deeper: imports that were `./lib/X.svelte` become `./X.svelte` for the components, `./Icon.svelte` etc. stay `./` (they live in `lib/`), and `./assets/logo.png` becomes `./../assets/logo.png` (i.e. `../assets/logo.png`). `./lib/reveal.js` → `./reveal.js`. Verify every import resolves.

- [ ] **Step 4: Rewrite `App.svelte` as the router switch (with a temporary docs stub)**

```svelte
<script>
  import { route } from './lib/docs/router.js'
  import Landing from './lib/Landing.svelte'
</script>

{#if $route.name === 'docs'}
  <div style="padding:80px;color:var(--text);font-family:var(--mono)">
    docs stub — slug: {$route.slug}
  </div>
{:else}
  <Landing />
{/if}
```

- [ ] **Step 5: Build**

```bash
cd site && npm run build
```
Expected: clean, zero warnings.

- [ ] **Step 6: Screenshot both routes**

Landing at `http://localhost:5173/#/` — must look identical to before (hero, sections, footer). Docs stub at `http://localhost:5173/#/docs/quickstart` — shows "docs stub — slug: quickstart". Confirm a bare anchor like `http://localhost:5173/#sync` still scrolls the landing (route stays landing).

- [ ] **Step 7: Commit**

```bash
git add site/package.json site/package-lock.json site/vite.config.js site/src/lib/docs/router.js site/src/lib/Landing.svelte site/src/App.svelte
git commit -m "docs-reader: hash router, extract Landing, add marked"
```

---

## Task 2: Markdown pipeline + nav config + MarkdownView

**Files:**
- Create: `site/src/lib/docs/nav.js`
- Create: `site/src/lib/docs/markdown.js`
- Create: `site/src/lib/docs/MarkdownView.svelte`
- Modify: `site/src/App.svelte` (render MarkdownView in the docs branch, temporarily, to verify)

**Interfaces:**
- Consumes: `marked`.
- Produces: `nav.js` exports `docs` (`{[slug]: {title, raw}}`) and `sections` (`[{title, items:[slug]}]`).
- Produces: `markdown.js` exports `renderDoc(md:string) → {html:string, toc:[{id,text,level}]}` and `buildSearchIndex() → [{slug, title, heading:string|null, id:string|null, text:string}]`.
- Produces: `MarkdownView.svelte` — prop `slug:string`; renders the doc and dispatches/binds its `toc`.

- [ ] **Step 1: Create `nav.js`**

The path from `site/src/lib/docs/` up to the repo `docs/` is four levels (`docs→lib→src→site→naht`):

```js
import quickstart from '../../../../docs/quickstart.md?raw'
import architecture from '../../../../docs/architecture.md?raw'
import priorArt from '../../../../docs/prior-art.md?raw'
import spec from '../../../../docs/spec.md?raw'

export const docs = {
  quickstart: { title: 'Quickstart', raw: quickstart },
  architecture: { title: 'Architecture', raw: architecture },
  'prior-art': { title: 'Prior art', raw: priorArt },
  spec: { title: 'Spec', raw: spec },
}

export const sections = [
  { title: 'Getting Started', items: ['quickstart'] },
  { title: 'Concepts', items: ['architecture', 'prior-art'] },
  { title: 'Reference', items: ['spec'] },
]
```

- [ ] **Step 2: Create `markdown.js`**

```js
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
```

- [ ] **Step 3: Create `MarkdownView.svelte`**

```svelte
<script>
  import { renderDoc } from './markdown.js'
  import { docs } from './nav.js'

  export let slug
  export let toc = []   // bindable: parent reads the active doc's TOC

  $: doc = docs[slug]
  $: rendered = doc ? renderDoc(doc.raw) : { html: '', toc: [] }
  $: toc = rendered.toc
</script>

{#if doc}
  <article class="prose">{@html rendered.html}</article>
{:else}
  <article class="prose"><h2>Not found</h2><p>No doc named “{slug}”.</p></article>
{/if}

<style>
  .prose { max-width: 76ch; color: var(--text); line-height: 1.7; }
  .prose :global(h1) { font-size: 2rem; letter-spacing: -0.02em; margin: 0 0 18px; }
  .prose :global(h2) { font-size: 1.4rem; margin: 38px 0 12px; letter-spacing: -0.01em; }
  .prose :global(h3) { font-size: 1.1rem; margin: 26px 0 10px; }
  .prose :global(p), .prose :global(li) { color: var(--text-dim); }
  .prose :global(a) { color: var(--studio); text-decoration: none; }
  .prose :global(a:hover) { text-decoration: underline; }
  .prose :global(code) {
    font-family: var(--mono); font-size: 0.88em; color: var(--fs);
    background: var(--bg-soft); border: 1px solid var(--border); border-radius: 5px; padding: 1px 5px;
  }
  .prose :global(pre) {
    background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-sm);
    padding: 14px 16px; overflow-x: auto;
  }
  .prose :global(pre code) { background: none; border: none; color: var(--text); padding: 0; }
  .prose :global(blockquote) {
    border-left: 2px solid var(--border-strong); margin: 16px 0; padding: 2px 16px; color: var(--text-faint);
  }
  .prose :global(table) { border-collapse: collapse; width: 100%; margin: 16px 0; }
  .prose :global(th), .prose :global(td) {
    border: 1px solid var(--border); padding: 8px 12px; text-align: left; font-size: 0.92rem;
  }
  .prose :global(.callout) {
    border: 1px solid var(--border-strong); border-radius: var(--radius-sm);
    background: var(--bg-soft); padding: 12px 16px; margin: 18px 0;
  }
  .prose :global(.callout.note) { border-left: 3px solid var(--merge); }
  .prose :global(.callout.tip) { border-left: 3px solid var(--ok); }
  .prose :global(.callout.warning), .prose :global(.callout.important) { border-left: 3px solid var(--fs); }
  .prose :global(.callout > :first-child) { margin-top: 0; }
  .prose :global(.callout > :last-child) { margin-bottom: 0; }
</style>
```

- [ ] **Step 4: Temporarily render MarkdownView in `App.svelte` docs branch**

Replace the docs stub `<div>` from Task 1 with:

```svelte
{#if $route.name === 'docs'}
  <div style="max-width:900px;margin:0 auto;padding:60px 24px">
    <MarkdownView slug={$route.slug} />
  </div>
{:else}
  <Landing />
{/if}
```
Add `import MarkdownView from './lib/docs/MarkdownView.svelte'` to the script. (This temporary wrapper is replaced by DocsLayout in Task 3.)

- [ ] **Step 5: Build**

```bash
cd site && npm run build
```
Expected: clean, zero warnings.

- [ ] **Step 6: Screenshot a rendered doc**

`http://localhost:5173/#/docs/architecture` — confirm headings, paragraphs, lists, inline code, and a code block all render and are styled (mono code, bordered `pre`). Then `#/docs/quickstart` to confirm switching works.

- [ ] **Step 7: Commit**

```bash
git add site/src/lib/docs/nav.js site/src/lib/docs/markdown.js site/src/lib/docs/MarkdownView.svelte site/src/App.svelte
git commit -m "docs-reader: markdown pipeline, nav config, MarkdownView"
```

---

## Task 3: DocsLayout shell + Sidebar + TopBar

**Files:**
- Create: `site/src/lib/docs/DocsLayout.svelte`
- Create: `site/src/lib/docs/TopBar.svelte`
- Create: `site/src/lib/docs/Sidebar.svelte`
- Modify: `site/src/App.svelte` (render `<DocsLayout slug={$route.slug} />` for docs)

**Interfaces:**
- Consumes: `route`/`navigate` (router.js), `sections`/`docs` (nav.js), `MarkdownView`, `logo`.
- Produces: `DocsLayout.svelte` — prop `slug`. Hosts TopBar + Sidebar + MarkdownView (+ a placeholder right rail, filled in Task 4).
- Produces: `Sidebar.svelte` (prop `slug`), `TopBar.svelte` (no props; search button is a placeholder until Task 5).

- [ ] **Step 1: Create `TopBar.svelte`**

```svelte
<script>
  import logo from '../../assets/logo.png'
  const REPO = 'https://github.com/vskstudio/naht'
  export let onSearch = () => {}
</script>

<header class="topbar">
  <a class="brand" href="#/"><img src={logo} alt="" width="24" height="24" /> naht</a>
  <button class="search" on:click={onSearch}>
    <span>Search</span><kbd>Ctrl K</kbd>
  </button>
  <nav>
    <a href="#/">Home</a>
    <a href="#/docs/quickstart">Docs</a>
    <a href={REPO} target="_blank" rel="noreferrer">GitHub</a>
  </nav>
</header>

<style>
  .topbar {
    position: sticky; top: 0; z-index: 40; display: flex; align-items: center; gap: 20px;
    height: 56px; padding: 0 20px; border-bottom: 1px solid var(--border);
    background: rgba(7, 8, 9, 0.72); backdrop-filter: blur(12px);
  }
  .brand { display: inline-flex; align-items: center; gap: 8px; font-family: var(--mono); font-weight: 700; color: var(--text); }
  .brand img { image-rendering: pixelated; display: block; }
  .search {
    display: inline-flex; align-items: center; gap: 10px; margin-left: 8px;
    color: var(--text-faint); background: var(--bg-soft); border: 1px solid var(--border);
    border-radius: 8px; padding: 6px 12px; font-size: 0.85rem; cursor: pointer;
  }
  .search:hover { border-color: var(--border-strong); }
  .search kbd { font-family: var(--mono); font-size: 0.72rem; border: 1px solid var(--border); border-radius: 4px; padding: 1px 5px; }
  nav { margin-left: auto; display: flex; gap: 18px; }
  nav a { color: var(--text-dim); font-size: 0.9rem; }
  nav a:hover { color: var(--text); }
  @media (max-width: 720px) { .search span { display: none; } }
</style>
```

- [ ] **Step 2: Create `Sidebar.svelte`**

```svelte
<script>
  import { sections, docs } from './nav.js'
  export let slug
</script>

<aside class="sidebar">
  {#each sections as section}
    <div class="group">
      <div class="group-title">{section.title}</div>
      {#each section.items as item}
        <a class="link" class:active={item === slug} href={`#/docs/${item}`}>{docs[item].title}</a>
      {/each}
    </div>
  {/each}
</aside>

<style>
  .sidebar { padding: 26px 16px; }
  .group { margin-bottom: 22px; }
  .group-title {
    font-family: var(--mono); font-size: 0.72rem; letter-spacing: 0.06em; text-transform: uppercase;
    color: var(--text-faint); margin-bottom: 8px; padding: 0 10px;
  }
  .link { display: block; padding: 6px 10px; border-radius: 7px; color: var(--text-dim); font-size: 0.92rem; }
  .link:hover { color: var(--text); background: var(--bg-soft); }
  .link.active { color: var(--fs); background: var(--fs-soft); }
</style>
```

- [ ] **Step 3: Create `DocsLayout.svelte`**

```svelte
<script>
  import TopBar from './TopBar.svelte'
  import Sidebar from './Sidebar.svelte'
  import MarkdownView from './MarkdownView.svelte'
  export let slug
  let toc = []
</script>

<TopBar />
<div class="shell">
  <Sidebar {slug} />
  <main class="content"><MarkdownView {slug} bind:toc /></main>
  <div class="rail"><!-- TocRight lands here in Task 4 --></div>
</div>

<style>
  .shell {
    display: grid; grid-template-columns: 256px minmax(0, 1fr) 240px;
    max-width: 1380px; margin: 0 auto; align-items: start;
  }
  .content { padding: 40px 48px; min-width: 0; }
  .rail { padding: 40px 16px; }
  @media (max-width: 960px) { .shell { grid-template-columns: 256px minmax(0,1fr); } .rail { display: none; } }
  @media (max-width: 720px) { .shell { grid-template-columns: 1fr; } .content { padding: 28px 20px; } }
</style>
```

- [ ] **Step 4: Wire DocsLayout into `App.svelte`**

```svelte
<script>
  import { route } from './lib/docs/router.js'
  import Landing from './lib/Landing.svelte'
  import DocsLayout from './lib/docs/DocsLayout.svelte'
</script>

{#if $route.name === 'docs'}
  <DocsLayout slug={$route.slug} />
{:else}
  <Landing />
{/if}
```

- [ ] **Step 5: Build**

```bash
cd site && npm run build
```
Expected: clean, zero warnings.

- [ ] **Step 6: Screenshot + click-through**

`http://localhost:5173/#/docs/quickstart` — confirm: sticky top bar (logo, search button, Home/Docs/GitHub), left sidebar with the three groups, active item highlighted in amber, content in the center. Navigate `#/docs/architecture` and `#/docs/prior-art` and confirm the active highlight follows and content swaps.

- [ ] **Step 7: Commit**

```bash
git add site/src/lib/docs/TopBar.svelte site/src/lib/docs/Sidebar.svelte site/src/lib/docs/DocsLayout.svelte site/src/App.svelte
git commit -m "docs-reader: 3-column layout, sidebar, top bar"
```

---

## Task 4: Right "On this page" TOC + scrollspy

**Files:**
- Create: `site/src/lib/docs/TocRight.svelte`
- Modify: `site/src/lib/docs/DocsLayout.svelte` (mount TocRight with the bound `toc`)

**Interfaces:**
- Consumes: the `toc` array from `MarkdownView` (`[{id,text,level}]`).
- Produces: `TocRight.svelte` — prop `toc`; highlights the heading currently in view via IntersectionObserver.

- [ ] **Step 1: Create `TocRight.svelte`**

```svelte
<script>
  import { onMount } from 'svelte'
  export let toc = []
  let activeId = ''

  onMount(() => {
    let observer
    const wire = () => {
      observer?.disconnect()
      const targets = toc.map((t) => document.getElementById(t.id)).filter(Boolean)
      if (!targets.length) return
      observer = new IntersectionObserver(
        (entries) => {
          for (const e of entries) if (e.isIntersecting) activeId = e.target.id
        },
        { rootMargin: '0px 0px -70% 0px', threshold: 0 },
      )
      targets.forEach((el) => observer.observe(el))
    }
    wire()
    return () => observer?.disconnect()
  })

  // Re-wire when the toc changes (doc switched).
  $: toc, typeof document !== 'undefined' && queueMicrotask(() => {
    activeId = ''
  })
</script>

{#if toc.length}
  <nav class="toc">
    <div class="toc-title">On this page</div>
    {#each toc as t}
      <a class="toc-link" class:sub={t.level === 3} class:active={t.id === activeId} href={`#${t.id}`}>{t.text}</a>
    {/each}
  </nav>
{/if}

<style>
  .toc { position: sticky; top: 76px; }
  .toc-title { font-family: var(--mono); font-size: 0.72rem; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 10px; }
  .toc-link { display: block; padding: 3px 0; color: var(--text-dim); font-size: 0.85rem; border-left: 2px solid transparent; padding-left: 12px; }
  .toc-link.sub { padding-left: 24px; }
  .toc-link:hover { color: var(--text); }
  .toc-link.active { color: var(--fs); border-left-color: var(--fs); }
</style>
```

Note: changing the doc remounts `MarkdownView` content; because `TocRight` lives in `DocsLayout` and `toc` is reactive, the IntersectionObserver must re-wire when `toc` changes. The simplest robust approach is to key the component on the slug (next step) so it re-mounts per doc.

- [ ] **Step 2: Mount TocRight in DocsLayout, keyed on slug**

In `DocsLayout.svelte`, import TocRight and replace the `.rail` placeholder. Key it on `slug` so the observer re-wires cleanly per doc:

```svelte
<script>
  import TopBar from './TopBar.svelte'
  import Sidebar from './Sidebar.svelte'
  import MarkdownView from './MarkdownView.svelte'
  import TocRight from './TocRight.svelte'
  export let slug
  let toc = []
</script>

<TopBar />
<div class="shell">
  <Sidebar {slug} />
  <main class="content">{#key slug}<MarkdownView {slug} bind:toc />{/key}</main>
  <div class="rail">{#key slug}<TocRight {toc} />{/key}</div>
</div>
```
(Keep the existing `<style>` block.)

- [ ] **Step 3: Build**

```bash
cd site && npm run build
```
Expected: clean, zero warnings.

- [ ] **Step 4: Screenshot scrollspy**

`http://localhost:5173/#/docs/architecture` at `--window-size=1440,1024` — confirm the right "On this page" lists the h2/h3 of the doc. Then screenshot at a scrolled position by appending a heading anchor, e.g. `#/docs/architecture#data-flow` won't scroll in headless; instead take a tall `--window-size=1440,2200` shot and confirm the first in-view heading is highlighted (amber, left border). Confirm the rail is hidden at `--window-size=900,1024`.

- [ ] **Step 5: Commit**

```bash
git add site/src/lib/docs/TocRight.svelte site/src/lib/docs/DocsLayout.svelte
git commit -m "docs-reader: on-this-page TOC with scrollspy"
```

---

## Task 5: Ctrl+K search

**Files:**
- Create: `site/src/lib/docs/Search.svelte`
- Modify: `site/src/lib/docs/DocsLayout.svelte` (mount Search, wire the TopBar button + Ctrl/Cmd+K)

**Interfaces:**
- Consumes: `buildSearchIndex()` (markdown.js), `navigate()` (router.js).
- Produces: `Search.svelte` — `bind:open`; filters the index; Enter/click navigates to `#/docs/<slug>#<id>`.

- [ ] **Step 1: Create `Search.svelte`**

```svelte
<script>
  import { onMount } from 'svelte'
  import { buildSearchIndex } from './markdown.js'
  import { navigate } from './router.js'

  export let open = false
  let q = ''
  let sel = 0
  const index = buildSearchIndex()

  $: results = q.trim()
    ? index.filter((e) => e.text.toLowerCase().includes(q.trim().toLowerCase())).slice(0, 12)
    : index.filter((e) => e.heading === null) // show doc titles when empty
  $: if (results) sel = 0

  function go(e) {
    if (!e) return
    navigate(e.id ? `#/docs/${e.slug}#${e.id}` : `#/docs/${e.slug}`)
    open = false
    q = ''
  }
  function onKey(ev) {
    if (!open) return
    if (ev.key === 'Escape') open = false
    else if (ev.key === 'ArrowDown') { sel = Math.min(sel + 1, results.length - 1); ev.preventDefault() }
    else if (ev.key === 'ArrowUp') { sel = Math.max(sel - 1, 0); ev.preventDefault() }
    else if (ev.key === 'Enter') { go(results[sel]); ev.preventDefault() }
  }
  onMount(() => {
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })
</script>

{#if open}
  <div class="overlay" on:click={() => (open = false)} role="presentation">
    <div class="modal" on:click|stopPropagation role="dialog" aria-label="Search docs">
      <!-- svelte-ignore a11y-autofocus -->
      <input autofocus placeholder="Search docs…" bind:value={q} />
      <ul>
        {#each results as r, i}
          <li class:sel={i === sel} on:mouseenter={() => (sel = i)} on:click={() => go(r)}>
            <span class="doc">{r.title}</span>{#if r.heading}<span class="sep">›</span><span class="head">{r.heading}</span>{/if}
          </li>
        {/each}
        {#if !results.length}<li class="empty">No matches</li>{/if}
      </ul>
    </div>
  </div>
{/if}

<style>
  .overlay { position: fixed; inset: 0; z-index: 60; background: rgba(0,0,0,0.55); display: flex; justify-content: center; align-items: flex-start; padding-top: 12vh; }
  .modal { width: min(560px, 92vw); background: var(--bg-card); border: 1px solid var(--border-strong); border-radius: var(--radius); box-shadow: var(--shadow); overflow: hidden; }
  input { width: 100%; padding: 16px 18px; background: none; border: none; border-bottom: 1px solid var(--border); color: var(--text); font-size: 1rem; outline: none; }
  ul { list-style: none; margin: 0; padding: 6px; max-height: 50vh; overflow-y: auto; }
  li { display: flex; align-items: center; gap: 8px; padding: 9px 12px; border-radius: 8px; color: var(--text-dim); cursor: pointer; font-size: 0.9rem; }
  li.sel { background: var(--bg-soft); color: var(--text); }
  .doc { color: var(--text); }
  .sep { color: var(--text-faint); }
  .head { color: var(--text-dim); }
  .empty { color: var(--text-faint); cursor: default; }
  @media (prefers-reduced-motion: reduce) { .modal { animation: none; } }
</style>
```

- [ ] **Step 2: Wire Search + Ctrl/Cmd+K in DocsLayout**

In `DocsLayout.svelte`:
- Import `Search`.
- Add `let searchOpen = false`.
- Pass `onSearch={() => (searchOpen = true)}` to `<TopBar>`.
- Add a window keydown handler for Ctrl/Cmd+K that toggles `searchOpen` (preventDefault).
- Mount `<Search bind:open={searchOpen} />`.

```svelte
<script>
  import { onMount } from 'svelte'
  import TopBar from './TopBar.svelte'
  import Sidebar from './Sidebar.svelte'
  import MarkdownView from './MarkdownView.svelte'
  import TocRight from './TocRight.svelte'
  import Search from './Search.svelte'
  export let slug
  let toc = []
  let searchOpen = false
  onMount(() => {
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); searchOpen = !searchOpen }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })
</script>

<TopBar onSearch={() => (searchOpen = true)} />
<div class="shell">
  <Sidebar {slug} />
  <main class="content">{#key slug}<MarkdownView {slug} bind:toc />{/key}</main>
  <div class="rail">{#key slug}<TocRight {toc} />{/key}</div>
</div>
<Search bind:open={searchOpen} />

<!-- keep the existing <style> block -->
```

- [ ] **Step 3: Build**

```bash
cd site && npm run build
```
Expected: clean, zero warnings.

- [ ] **Step 4: Verify the modal**

Headless Chrome can't press Ctrl+K. Temporarily set `let searchOpen = true` (or `export let open = true` default in Search) to screenshot the open modal at `#/docs/quickstart`, confirm it lists doc titles and that typing would filter (read the filter logic). Then REVERT the temporary default to `false`. Note in the report that the open/close keybinding was verified by code review + the forced-open screenshot.

- [ ] **Step 5: Commit**

```bash
git add site/src/lib/docs/Search.svelte site/src/lib/docs/DocsLayout.svelte
git commit -m "docs-reader: Ctrl+K client search"
```

---

## Task 6: Sober styling, responsive drawer, a11y, final sweep

**Files:**
- Modify: `site/src/lib/docs/DocsLayout.svelte` (mobile sidebar drawer toggle)
- Modify: `site/src/lib/docs/Sidebar.svelte` / `TopBar.svelte` (drawer affordance)
- Modify: any docs component needing polish, reduced-motion, or overflow fixes

**Interfaces:** Consumes everything above; produces the finished, coherent reader.

- [ ] **Step 1: Mobile sidebar drawer**

Below 720px the left sidebar should be hidden by default and toggled by a hamburger button in the TopBar. Implement: add a `menuOpen` state in `DocsLayout`, pass an `onMenu` handler + `open` state to a small toggle in `TopBar` (a button visible only ≤720px), and give `Sidebar` an `open` prop that, on mobile, slides it in as an overlay drawer (`position: fixed; inset: 56px 0 0 0; background: var(--bg);` with a translateX transform; gated behind `@media (max-width: 720px)`). Above 720px the sidebar is always shown and the toggle is hidden.

- [ ] **Step 2: Reduced-motion + a11y pass**

Ensure any transition (drawer slide, search modal, hover) has an off-switch under `@media (prefers-reduced-motion: reduce)`. Confirm: the search input is focusable/Esc-closable; the active sidebar/TOC items have sufficient contrast; decorative logo carries `alt=""`. Verify with `--force-prefers-reduced-motion` that the drawer/modal render their end state and nothing is stuck invisible.

- [ ] **Step 3: Dead-code/import sweep + sober consistency**

Confirm no unused imports/selectors across the new `lib/docs/*` files (build must be warning-free). Check the docs view stays sober: no big gradient surfaces, accent only on links/active states/search highlight. Tweak spacing for an even rhythm.

- [ ] **Step 4: Responsive + desktop screenshots**

- Desktop `--window-size=1440,1024` at `#/docs/architecture`: 3 columns, sidebar + content + TOC, sober palette.
- Mid `--window-size=900,1024`: right TOC hidden, sidebar + content.
- Mobile `--window-size=390,1600`: sidebar collapsed to a drawer (closed by default), content full width, no horizontal overflow (code blocks/tables scroll within their box).
- Landing still intact at `#/`.

- [ ] **Step 5: Final clean build**

```bash
cd site && npm run build
```
Expected: **zero warnings**.

- [ ] **Step 6: Commit**

```bash
git add site/src
git commit -m "docs-reader: responsive drawer, reduced-motion, sober polish"
```

---

## Self-Review

**Spec coverage:**
- Extend `site/`, landing at `#/`, docs at `#/docs/*` → Task 1 (router + Landing). ✔
- Render `docs/*.md` via marked, single source → Task 2 (nav + markdown + MarkdownView). ✔
- Heading ids, TOC, `.md`→`#/docs` link rewrite, callouts → Task 2 (`renderDoc`). ✔
- 3-column layout, collapsible sidebar with active state, top bar → Task 3. ✔
- "On this page" scrollspy → Task 4. ✔
- Ctrl+K client search over titles + headings → Task 5. ✔
- Sober register, responsive (TOC hides <960, sidebar drawer <720, no overflow), reduced-motion → Task 6 (and per-component styles). ✔
- Only new dep `marked`; `base:'./'`; `server.fs.allow` for `../docs` → Global Constraints + Task 1. ✔

**Placeholder scan:** No TBD/TODO; each code step shows complete code. The one "fill in" (mobile drawer in Task 6 Step 1) is described as concrete behavior with the exact CSS approach — acceptable as a styling step verified by screenshot, consistent with the site's screenshot-driven gate.

**Type/name consistency:** `route` store shape `{name, slug}` used identically in App/DocsLayout; `renderDoc → {html, toc}` and `toc` item shape `{id,text,level}` consistent across markdown.js, MarkdownView, TocRight, Search; `buildSearchIndex` item shape `{slug,title,heading,id,text}` consistent in Search; `navigate()`/`docs`/`sections` signatures consistent. ✔
