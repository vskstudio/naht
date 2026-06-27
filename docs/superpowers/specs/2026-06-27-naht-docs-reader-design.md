# Naht docs reader — design

**Date:** 2026-06-27
**Status:** Approved design, pending implementation plan
**Scope:** Extend the existing `site/` (Svelte 5 + Vite) with a sober, Satvrn-style
documentation reader at `/docs/*`, rendering the repo's existing markdown. Reuse the naht color
tokens and logo. No daemon/CLI/plugin changes.

## Goal

Add a calm, readable three-column documentation reader (sidebar · content · "On this page" TOC) in
the sober style of the Satvrn reference, using the naht palette in a restrained register. The repo's
`docs/*.md` are the single source of truth — the reader renders them, it does not duplicate copy.

## Decisions locked during brainstorming

- **Structure:** extend the existing `site/` — landing stays at `/`, docs reader lives at `/docs/*`.
  Reuse the existing tokens (`app.css`), logo, and dark palette.
- **Register:** sober. Restrained gradient (no big gradient hero in the docs view), neutral
  surfaces, accent (amber primary; periwinkle/cyan for links + active nav) used sparingly.
- **Routing:** hash-based (`#/`, `#/docs/<slug>`) so it works under `base:'./'` static hosting with
  no server rewrites.
- **Content:** render the existing `docs/*.md` (quickstart, architecture, prior-art, spec) via Vite
  `?raw` import + `marked`. Single source of truth.
- **Search:** include a simple client-side Ctrl+K search (filters doc titles + headings).
- **PR:** new branch `feat/docs-reader` off `feat/docs-site`; PR targets `feat/docs-site` (stacked,
  diff shows only the reader).

## Architecture

`App.svelte` becomes a thin router switch over a hash route:

- `#/` (and empty hash) → `Landing.svelte` (the current landing, extracted verbatim from today's
  `App.svelte`).
- `#/docs` → redirect to the first doc (`#/docs/quickstart`).
- `#/docs/<slug>` → `DocsLayout.svelte` with the matching doc.

The landing's existing in-page anchors (`#why`, `#sync`, …) keep working because hash routing only
treats `#/`-prefixed hashes as routes; bare `#section` anchors on the landing are unaffected (the
router ignores hashes that don't start with `#/`).

## Content pipeline

- The four docs are imported raw at build: `import quickstart from '../../docs/quickstart.md?raw'`
  (and architecture, prior-art, spec). Vite serves `?raw` as a string; the files stay the single
  source of truth in `naht/docs/`.
- `marked` converts markdown → HTML. A small post-step assigns slugified `id`s to `h2`/`h3`
  headings (for TOC anchors + scrollspy) and rewrites relative `.md` links to `#/docs/<slug>`.
- Code blocks: rendered in mono, styled (background, border), **no syntax highlighting in v1**
  (keeps deps lean; can add later).
- Callouts: a blockquote starting with `> [!NOTE]` / `> [!WARNING]` renders as a styled callout box
  (the Satvrn purple-info-box equivalent, in our palette). Plain blockquotes render normally.

## Layout — docs reader (3 columns)

- **Top bar:** logo + "naht" wordmark · nav (Home → `#/`, Docs, GitHub) · a search affordance
  (button showing `Ctrl K`). Sober, sticky, blurred — same nav language as the landing but calmer.
- **Left sidebar:** collapsible sections from a small `nav.js` config —
  *Getting Started* (Quickstart) · *Concepts* (Architecture, Prior art) · *Reference* (Spec). The
  active item is highlighted in accent. Sections collapse/expand.
- **Center:** the rendered markdown in a readable max-width column, with heading anchors, styled
  lists/tables/code/callouts.
- **Right "On this page":** h2/h3 of the active doc, auto-extracted, with scrollspy highlighting the
  section currently in view. Hidden below a width threshold.
- **Responsive:** below ~960px the right TOC hides; below ~720px the left sidebar becomes a toggled
  drawer and content goes full width. No horizontal overflow (code/tables scroll within their box).

## Search (Ctrl+K)

A lightweight client-side modal:

- Opens on `Ctrl/Cmd+K` (and a top-bar button).
- Index = each doc's title + its h2/h3 headings (built once from the parsed docs).
- Typing filters the index; Enter / click navigates to `#/docs/<slug>#<heading-id>`.
- Esc closes; arrow keys move the selection. No fuzzy library — simple case-insensitive substring
  match is enough for four docs.

## Components (`site/src/lib/docs/`)

| File | Role | Depends on |
|---|---|---|
| `router.js` | Tiny hash router: parse `location.hash`, expose a Svelte store `route`, `navigate()` | — |
| `nav.js` | Sidebar config (sections → slugs/titles) and the slug→raw-markdown map | the `?raw` doc imports |
| `markdown.js` | `marked` setup: render to HTML, assign heading ids, extract `{id,text,level}` TOC, rewrite `.md` links, parse callouts | `marked` |
| `DocsLayout.svelte` | 3-column shell: TopBar + Sidebar + content + TocRight | the four below |
| `TopBar.svelte` | Logo, nav, search button | `router`, `Search` |
| `Sidebar.svelte` | Collapsible section nav, active state | `nav`, `router` |
| `MarkdownView.svelte` | Renders the active doc's HTML; emits its TOC | `markdown` |
| `TocRight.svelte` | "On this page" list + scrollspy | — |
| `Search.svelte` | Ctrl+K modal over the heading index | `nav`, `markdown`, `router` |
| `Landing.svelte` | The current landing, extracted from `App.svelte` | existing landing components |

`App.svelte` shrinks to: import the router store + `Landing`/`DocsLayout`, and switch on `route`.

## Colors / sober register

Reuse the existing `app.css` tokens. The docs view leans on neutral surfaces (`--bg`, `--bg-soft`,
`--bg-card`, `--border`), with accent only on: links (periwinkle/cyan), the active sidebar item and
active TOC item (amber/periwinkle), and the search highlight. The signature gradient appears at most
on the wordmark mark — not as a hero. This is intentionally calmer than the landing.

## Build, hosting, verification

- Add `marked` as the only new dependency. Keep `base: './'`.
- `vite.config.js` may need `server.fs.allow` / no special config for `?raw` imports of `../../docs`
  — confirm the relative import resolves; if Vite blocks the parent path, add the docs dir to
  `server.fs.allow` (dev) — build-time `?raw` imports resolve regardless.
- Definition of done per step: `npm run build` clean (zero warnings).
- Screenshot-verify: docs reader desktop (3 columns), a doc with a callout + code block, the Ctrl+K
  search modal, and mobile (sidebar drawer, TOC hidden, no overflow).

## Out of scope

- No syntax highlighting (v1).
- No full-text content search (titles + headings only).
- No versioned docs, no i18n, no edit-on-GitHub links (could come later).
- No changes to `naht/` (daemon, CLI, plugin) or the docs' content.

## Risks / notes

- Importing `../../docs/*.md?raw` couples the site build to the repo docs location; if a doc is
  renamed, update `nav.js`. This is the price of a single source of truth and is acceptable.
- The hash router must not hijack the landing's bare in-page anchors — only `#/`-prefixed hashes are
  routes.
- Keep `marked` output sanitized/trusted: the docs are first-party repo content, so XSS is not a
  concern, but disable any raw-HTML passthrough surprises by using marked defaults.
