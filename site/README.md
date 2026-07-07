# Naht docs site

A single-page documentation site for Naht, built with **Svelte 5 (runes) + Vite**. The page leads
with a Naht/Rojo/Argon comparison matrix, then proves each differentiator with a concrete,
real-looking artifact (a VS Code merge editor, a SQLite state inspector, a `naht serve -vv` log
stream, a Cargo workspace tree with `cargo test`, and a guided terminal session). Inline-SVG icons
(no icon-font dependency) and scroll-reveal animations that respect `prefers-reduced-motion`.

The content is **bilingual (EN / FR)** via a small dependency-free i18n layer, and mirrors the repo
docs — [`../README.md`](../README.md), [`../docs/architecture.md`](../docs/architecture.md),
and [`../docs/quickstart.md`](../docs/quickstart.md) — so it stays a faithful surface over the
project.

## Develop

```sh
cd site
npm install
npm run dev        # http://localhost:5173
```

## Build & test

```sh
npm run build      # static output in site/dist/
npm run preview    # serve the built site locally
npm test           # Vitest: i18n logic + EN/FR dictionary parity
```

`vite.config.js` sets `base: './'`, so `dist/` works from any path — GitHub Pages subpaths, a static
host, or opened directly. It also holds the Vitest config (jsdom environment).

## Production (naht.dev)

The site is served **straight from disk** by the shared `edge` nginx reverse proxy (separate repo,
`EDGE_SITE=white`) — there is **no per-site container** at runtime, no host port, and zero idle RAM.
The edge terminates TLS, enforces the Cloudflare-origin lock-down, and sets all security response
headers. **There is intentionally no deploy script, no webhook, and no CI publish step**: the build
is run by hand on the host.

```sh
# on the white production host, in /home/naht.dev (this directory)
docker compose up --build     # builds, copies ./dist onto the host, container exits 0
```

The edge mounts `/home/naht.dev/dist` read-only as `/var/www/naht` and serves it; rebuild to
publish a new version and the edge picks it up immediately (no reload needed).

- `Dockerfile` — **build-only** image (Node, pinned by digest). It builds the Vite SPA and
  pre-compresses every text asset (`gzip -9 -k`) so the edge serves the `.gz` via `gzip_static` with
  no request-time CPU. There is no runtime server stage.
- `docker-compose.yml` — a disposable `naht_build` service: it copies `/app/dist` (with the
  pre-compressed `.gz` alongside) onto the host `./dist`, then exits. No `web` network, no exposed
  port.
- `vite.config.js` sets `build.sourcemap: false` so no original source or local paths ship.

The edge vhost (`nginx/white.conf` in the edge repo) owns the SPA fallback to `index.html`,
immutable caching for hashed assets, security headers, and the `naht/origin.{pem,key}` Cloudflare
Origin cert. Moving hosts also requires the naht.dev Cloudflare DNS to point at the white origin and
Authenticated Origin Pulls to stay enabled on the zone.

CI (`.github/workflows/ci.yml`) verifies the image builds and runs a Trivy scan (fails on
HIGH/CRITICAL), but never pushes it anywhere. TLS, the Cloudflare-origin lock-down, and DNS are
wired up at the edge (private infra), not described here.

## Internationalization

Default locale is **`en`**; the FR/EN toggle in the nav persists to `localStorage` and keeps
`<html lang>` in sync. First-visit locale falls back to `navigator.language`, then `en`. All
user-visible copy — including artifact contents — lives in the dictionaries, so both languages share
one set of markup.

## Structure

| File | Role |
|---|---|
| `src/App.svelte` | Page composition: nav, hero, the six numbered blocks, footer |
| `src/i18n/index.js` | `locale` store, `t` derived dictionary, detection + persistence |
| `src/i18n/en.js`, `src/i18n/fr.js` | String dictionaries (same key shape) |
| `src/lib/Hero.svelte`, `SeamPanel.svelte` | Hero copy + the floating disk⇄Studio "seam" visual |
| `src/lib/ComparisonMatrix.svelte` | 01 — Naht/Rojo/Argon table |
| `src/lib/MergeEditor.svelte` | 02 — VS Code 3-way merge editor |
| `src/lib/StateInspector.svelte` | 03 — SQLite `base` table inspector |
| `src/lib/LogStream.svelte` | 04 — `naht serve -vv` colored log stream |
| `src/lib/WorkspaceTree.svelte` | 05 — Cargo workspace tree + `cargo test` ribbon |
| `src/lib/GuidedTerminal.svelte` | 06 — scrolling `init → serve → synced` session |
| `src/lib/FeatureRow.svelte` | Copy + artifact layout wrapper (snippets, optional flip) |
| `src/lib/LanguageToggle.svelte` | FR/EN nav switch |
| `src/lib/Icon.svelte` | Inline-SVG icon set (stroke, `currentColor`) |
| `src/lib/reveal.js` | Scroll-reveal Svelte action (IntersectionObserver) |
| `src/app.css` | Design tokens, theme, base styles |
