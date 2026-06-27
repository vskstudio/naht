# Naht site redesign v2 — design

Date: 2026-06-27
Status: approved (mockup validated section by section in the visual companion)

## Goal

Rework the landing page to be cleaner and to **convince against Rojo and Argon**. The
page must differentiate Naht from the incumbents: lead with a synthetic comparison
matrix (the claim), then prove each differentiator with a concrete, real-looking
artifact (the proof), then let the reader try it.

Add **internationalization (FR + EN)** as part of this work.

Keep the existing dark visual identity and tri-color accent palette:

- `--bg:#070809`, `--fs:#f5b54a` (amber = filesystem), `--studio:#5cc8ff` (cyan = Studio),
  `--merge:#9aa0ff` (periwinkle = merge), `--ok:#4ad295`, `--danger:#f2616a`
- signature gradient `linear-gradient(110deg,var(--fs),var(--merge) 52%,var(--studio))`
- page max-width `1440px`, fonts Inter + JetBrains Mono.

The approved full-page mockup is
`.superpowers/brainstorm/33435-1782525587/content/fullpage-v8.html`. It is the visual
source of truth for layout, spacing, and the per-section artifacts.

## Page structure

The page tightened from the old 8-section feature dump to 7 focused blocks, removing
redundancy (the old 6-card "failure modes" grid was dropped — its content lives in the
matrix and the four proof blocks).

1. **Hero** — the signature "seam" visual. Two floating panes (a code editor on the
   filesystem side, a Studio Explorer tree on the Studio side) connected by curved SVG
   sutures, with `.lua` chips riding the curves via `offset-path`, a radial glow behind,
   and a status line below ("Connected · base ack-gated · re-diff au reconnect"). No
   outer container block; the panes float with tinted shadows. Headline uses the
   gradient on the word "seam"/"couture".

2. **01 Comparatif (matrix)** — Naht / Rojo / Argon comparison table. Naht column is
   accented with the gradient top border and a faint amber tint. Rows: bidirectional
   sync, 3-way text merge, persisted last-sync base, reconnect + visible state,
   non-destructive (zero unwrap), config by convention + layered, non-syncable props
   surfaced. Source note: `docs/prior-art.md`.

3. **02 Merge 3-way** — full-width **VS Code merge-editor** artifact. Two input panes on
   top (DISK = current/amber, STUDIO = incoming/cyan), a RESULT pane below with the
   `base last-sync · SQLite` reminder, a decorated conflict region (bar with
   Accept Disk / Accept Studio / Both actions, colored `<<<<<<< / ======= / >>>>>>>`
   markers), one auto-merged clean hunk, and a footer "1 hunk propre → base avance ·
   1 conflit → path gelé, jamais auto-résolu". Title + intro stacked above.

4. **03 Base persistée** — feature row, artifact = **SQLite inspector** window. A table
   on the `base` table (`path · blob_hash · rev · synced_at`, 4 rows, most-recent row
   tinted), header `base · .naht/state.db` + the query, footer "daemon restarted · base
   rechargée depuis le disque · re-diff 142 paths → 0 re-clobber". Copy on the other side.

5. **04 Non-destructif** — feature row, artifact = **`naht serve -vv` log stream**.
   Colored timestamped lines with INFO/WARN/ERR/OK levels: write fail → lane FS→Studio
   paused (session intact) → Studio→FS continues → connection lost → reconnect backoff
   1s→2s→4s → reconnected, re-diff, lane resumed · 0 loss. Blinking cursor at the end.

6. **05 Architecture** — feature row, artifact = **Cargo workspace** window. Repo tree
   on the left (`naht-core/` with `0 I/O · testable` badge, `naht/` daemon `owns I/O`,
   `plugin/` Luau `thin`, `Cargo.toml workspace`), and on the right the `Cargo.toml`
   `[workspace] members` plus a `cargo test -p naht-core → ✓ 38 passed; 0 failed —
   sans démarrer de serveur` ribbon. Proves "no I/O = testable".

7. **06 Essayer (try)** — a single **guided terminal session** that scrolls through
   `naht init demo → naht serve → daemon watching → plugin Connecting… → Connected →
   FS→Studio applied ✓ → Studio→FS merge 3-way → base #49 ✓ → ● bidirectionnel
   confirmé`. Below it: the **limits** as compact pills (MeshId/images upload, Terrain
   voxels, CSG/Unions round-trip, HttpEnabled/locked props hard block), then the two
   CTAs (full quickstart, architecture docs).

8. **Footer** — brand, tagline, links (GitHub, Quickstart, Architecture, Prior art),
   license line.

Section design principle: each proof block is a distinct *artifact type* (merge editor,
DB inspector, log stream, workspace tree, guided terminal) so nothing reads as repeated.

## Component plan (Svelte)

The current `site/src/lib` diagram components (`DataFlowDiagram`, `MergeDiagram`,
`ArchitectureDiagram`, `SeamPanel`, `BentoGrid`) map onto the new artifacts but need
rework. Target component set:

- `SeamPanel.svelte` — keep/upgrade to the approved floating-seam hero visual.
- `ComparisonMatrix.svelte` — new; renders the Naht/Rojo/Argon table from data.
- `MergeEditor.svelte` — new; the VS Code merge-editor artifact (replaces `MergeDiagram`).
- `StateInspector.svelte` — new; the SQLite table artifact.
- `LogStream.svelte` — new; the `naht serve -vv` colored log (replaces `DataFlowDiagram`).
- `WorkspaceTree.svelte` — new; repo tree + `cargo test` ribbon (replaces `ArchitectureDiagram`).
- `GuidedTerminal.svelte` — new; the scrolling session (replaces the CLI grid + steps).
- `FeatureRow.svelte` — keep; wraps copy + artifact slot with optional flip.
- `Card.svelte`, `Icon.svelte`, `reveal.js` — keep.

Each new component is presentational and data-driven: the rows/lines/tree nodes/log
entries come from props sourced from the i18n dictionaries (see below), so both
languages share one markup.

## Internationalization

Bespoke, dependency-free (the site is a single page; a library is overkill).

- `src/i18n/index.js` — exports a writable `locale` store, a derived `t` (or a `t(key)`
  helper), `setLocale(code)`, and locale init logic: read `localStorage.naht_locale`,
  else `navigator.language` prefix, else default **`en`**. Persist on change. Keep the
  `<html lang>` attribute in sync.
- `src/i18n/en.js`, `src/i18n/fr.js` — full string dictionaries, same key shape. Cover:
  nav, hero, all section eyebrows/titles/leads, matrix rows, merge-editor labels,
  inspector rows, log lines, workspace tree + test labels, guided-terminal lines,
  limits pills, CTAs, footer. The existing data arrays (`pains` is dropped; `commands`,
  `limits`, `steps` are reshaped into the new sections) move into these dictionaries.
- **Language toggle** in the nav (FR / EN), persisted, updates `<html lang>`.
- Default locale **`en`** (matches the current English site and a broad audience); FR is
  the full second locale (the mockup copy is the FR source).

All user-visible text — including artifact contents (log lines, table cells, terminal
output, code comments) — is keyed. Code identifiers, file paths, command names, and
hashes that are not prose stay literal.

## Out of scope

- No routing / multi-page; remains a single landing page.
- No CMS or runtime translation loading; dictionaries are bundled.
- No new visual identity; palette and gradient are unchanged.

## Acceptance

- The built site matches `fullpage-v8.html` section-for-section in structure and the
  per-section artifacts, within the existing palette.
- Toggling FR/EN swaps every visible string (including artifact contents) with no layout
  break, persists across reloads, and updates `<html lang>`.
- `prefers-reduced-motion` is honored for the animated bits (seam chips, log cursor,
  reveal-on-scroll).
- `npm run build` succeeds; the page works without JS errors.
