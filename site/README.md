# Naht docs site

A single-page documentation site for Naht, built with **Svelte 5 + Vite**. Animated SVG/CSS diagrams
(bidirectional flow, 3-way merge, architecture), inline-SVG icons (no icon-font dependency), and
scroll-reveal animations that respect `prefers-reduced-motion`.

The content mirrors the repo docs — [`../README.md`](../README.md),
[`../docs/architecture.md`](../docs/architecture.md), and
[`../docs/quickstart.md`](../docs/quickstart.md) — so it stays a faithful surface over the project.

## Develop

```sh
cd site
npm install
npm run dev        # http://localhost:5173
```

## Build

```sh
npm run build      # static output in site/dist/
npm run preview    # serve the built site locally
```

`vite.config.js` sets `base: './'`, so `dist/` works from any path — GitHub Pages subpaths, a static
host, or opened directly.

## Structure

| File | Role |
|---|---|
| `src/App.svelte` | Page composition: nav, hero, sections, footer |
| `src/lib/DataFlowDiagram.svelte` | Animated bidirectional sync schematic |
| `src/lib/MergeDiagram.svelte` | Clean 3-way merge vs frozen conflict |
| `src/lib/ArchitectureDiagram.svelte` | The three components and the transport boundary |
| `src/lib/Icon.svelte` | Inline-SVG icon set (stroke, `currentColor`) |
| `src/lib/reveal.js` | Scroll-reveal Svelte action (IntersectionObserver) |
| `src/app.css` | Design tokens, theme, base styles |
