# Naht docs site — visual redesign

**Date:** 2026-06-26
**Status:** Approved design, pending implementation plan
**Scope:** Purely visual redesign of the existing `site/` (Svelte 5 + Vite). No content
rewrite, no daemon/CLI changes.

## Goal

Bring the documentation site to a polished, premium "dev-tool" feel using **Linear** as the
benchmark: very dark surfaces, subtle radial glows, glass cards, a signature gradient, rigorous
grid, and refined micro-interactions. Keep the current message and the corrected logo; replace the
look.

## Decisions locked during brainstorming

- **Scope:** visual refonte only — keep Svelte 5 + Vite, single page, existing content sections,
  and the transparent pixel-art logo already fixed.
- **Benchmark:** Linear (dark, glow/glass, colored accent, ultra-refined micro-interactions).
- **Hero:** split layout — text + CTAs left, product visual right.
- **Hero right visual:** the "seam" (sync) panel — DISK ⇄ STUDIO with an animated dotted seam and a
  file flowing both ways. The on-brand choice that *shows* the metaphor.
- **Body rhythm:** hybrid — alternating full-width feature rows for the marquee concepts, plus a
  compact bento/grid for the dense lists.
- **Hero subline:** keep a single short line (not the removed paragraph).

## Design system

Centralize tokens in `site/src/app.css` (`:root`).

| Token group | Value / intent |
|---|---|
| Background | base `#070809`; soft `#0d0f14` |
| Card | `#0d0f14` surface, `#20232c` border, `#262a34` strong border |
| Text | primary `#eef0f4`, dim `#9aa0ab`, muted `#6b7280` |
| Signature gradient | filesystem amber `#f5b54a` → merge periwinkle `#9aa0ff` → studio cyan `#5cc8ff` |
| Accent roles | filesystem = amber, studio = cyan, merge = periwinkle, ok = `#4ad295` |
| Glow | radial blurs (~60px) behind hero/feature focal points, low opacity |
| Type | sans-serif with negative tracking on headings; mono (`--mono`) for pills, labels, code |
| Radius / shadow / motion | shared radius, layered shadows, and `--dur-*` animation durations as tokens |

The signature gradient is the site's identity: applied to the word "seam", section accent bars, and
hairline rules. One gradient, used sparingly.

## Page structure

Single page, sticky blurred nav, then:

1. **Hero** — split. Left: mono pill (`Rust · Roblox Studio · bidirectional`), gradient headline on
   two lines ("The *seam* between your filesystem / and Roblox Studio."), one short subline
   ("Bidirectional sync with a real 3-way merge. Never destructive."), two CTAs (primary amber
   **Quickstart**, ghost **See it sync**). Right: the **SeamPanel**.
2. **Sync** — alternating feature row. Reworked `DataFlowDiagram`. Both-directions-at-once, paused
   path on failed write, never kills the session.
3. **Merge** — alternating feature row (visual on the opposite side). Reworked `MergeDiagram`. Real
   3-way merge vs frozen conflict with git-style markers.
4. **Architecture** — alternating feature row. Reworked `ArchitectureDiagram`. `naht-core` / `naht`
   / `plugin` and the transport boundary.
5. **Failure modes** — compact bento/grid of the six pain→fix cards.
6. **CLI** — compact grid of the command list.
7. **Limits** — the syncability table (upload / syncable / round-trip / hard block).
8. **Quickstart** — numbered step frieze (the six steps).
9. **Footer** — brand (logo), links, license.

All copy is carried over from the current `App.svelte` constants (`pains`, `commands`, `limits`,
`steps`) — no content rewrite.

## Components (in `site/src/lib/`)

Each unit has one clear purpose, a small prop interface, and is verifiable in isolation.

| Component | Role | Depends on |
|---|---|---|
| `Hero.svelte` | Hero composition (left column + right slot) | `SeamPanel`, `Icon` |
| `SeamPanel.svelte` | Animated DISK ⇄ STUDIO seam with a file flowing both ways | CSS/SVG only |
| `FeatureRow.svelte` | Full-width alternating row: label, heading, body, diagram slot; `flip` prop swaps sides | `reveal` |
| `BentoGrid.svelte` + `Card.svelte` | Mixed-size card grid for failure modes / CLI | `Icon`, `reveal` |
| `DataFlowDiagram.svelte` | Restyled bidirectional sync schematic | — |
| `MergeDiagram.svelte` | Restyled 3-way merge vs frozen conflict | — |
| `ArchitectureDiagram.svelte` | Restyled three-component + transport boundary | — |
| `Icon.svelte` | Inline-SVG icon set (kept as-is) | — |
| `reveal.js` | Scroll-reveal action (kept) | IntersectionObserver |

`App.svelte` shrinks to page composition + the data constants; visual weight moves into the
components above. This keeps each file focused and easy to edit reliably.

## Motion & accessibility

- Scroll-reveal via `reveal.js` (IntersectionObserver), staggered per section.
- Subtle hover affordances: card lift + faint glow; CTA translateY.
- Animated seam (dotted stitch travel) and flow dots in `SeamPanel` and `DataFlowDiagram`.
- **`prefers-reduced-motion: reduce`** disables all transitions/keyframe motion and renders the
  final static state — nothing depends on motion to be understood.
- Text meets WCAG AA contrast on the dark surfaces. Decorative SVG/icons carry empty `alt`/`aria`.

## Build, hosting, verification

- `vite.config.js` keeps `base: './'` for static hosting (GitHub Pages subpaths included).
- Definition of done per step: `npm run build` is **clean (zero warnings)**.
- Visual verification with headless Chrome screenshots at each step: hero, each section, and a
  mobile width (≤720px) to confirm the two-line headline wraps gracefully and rows stack.

## Out of scope

- No content/copy rewrite (only carried over).
- No new sections or features beyond the current eight.
- No changes to `naht/` (daemon, CLI, plugin) or repo docs.
- No deployment pipeline (tracked separately if desired).

## Risks / notes

- The three diagrams are the most effort; restyle rather than rebuild where possible, preserving
  their current data/logic.
- The split hero must degrade to a single stacked column on narrow screens; the SeamPanel sits below
  the text on mobile.
- Keep the bundle lean — no icon-font or heavy animation library; SVG/CSS only.
