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

The site ships as a hardened static-file container fronted by the shared `edge` nginx reverse
proxy (separate repo) — the edge terminates TLS, enforces the Cloudflare-origin lock-down, rate
limits, and sets all security response headers. **There is intentionally no deploy script, no
webhook, and no CI publish step**: the production image is brought up by hand on the host.

```sh
# on the production host, in this directory
docker compose up -d --build
```

- `Dockerfile` — two-stage build (Node → `nginxinc/nginx-unprivileged`), both base images pinned by
  digest. The runtime runs **non-root (uid 101)** and binds the unprivileged port `8080`.
- `nginx.conf` — container-internal server: serves `dist/` on `:8080`, `/healthz` for the
  healthcheck, immutable caching for hashed `/assets/`, SPA fallback to `index.html`. It owns **no**
  TLS or security headers — the edge does.
- `docker-compose.yml` — joins the external `web` network as `naht_site:8080`, publishes **no host
  port**, and is hardened: `read_only` rootfs, `cap_drop: ALL`, `no-new-privileges`, tmpfs for
  `/tmp` + nginx cache.
- `vite.config.js` sets `build.sourcemap: false` so no original source or local paths ship.

CI (`.github/workflows/ci.yml`) verifies the image builds and runs a Trivy scan (fails on
HIGH/CRITICAL), but never pushes it anywhere. See [`SECURITY.md`](../SECURITY.md) for the full
production posture and the manual edge wiring (cert, DNS, Authenticated Origin Pulls).

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
