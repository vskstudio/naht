# Naht Site Visual Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-skin the existing `site/` to a polished, Linear-style dark aesthetic — split hero with an animated "seam" panel, hybrid body (alternating feature rows + bento), one tri-color signature gradient — without changing content, stack, or the daemon/CLI.

**Architecture:** Keep Svelte 5 + Vite, single page. Move visual weight out of the monolithic `App.svelte` `<style>` into focused, scoped components in `site/src/lib/`. The three diagrams are already token-driven, so most of their re-skin happens for free when the `:root` tokens in `app.css` change. `App.svelte` becomes page composition + the existing data constants (`pains`, `commands`, `limits`, `steps`).

**Tech Stack:** Svelte 5, Vite 5, plain CSS (design tokens in `app.css`), inline SVG/CSS animation (no new runtime deps).

## Global Constraints

- Stack stays **Svelte 5 + Vite**; no new **runtime** dependencies (dev-only is fine, but none are required here). SVG/CSS only — no icon font, no animation library.
- `site/vite.config.js` keeps `base: './'` (static hosting / GitHub Pages subpaths).
- Content is **carried over verbatim** from the current `App.svelte` constants and copy — no rewording.
- The corrected transparent logo at `site/src/assets/logo.png` is kept as-is.
- Signature gradient, used sparingly (word "seam", section accent bars, hairlines): filesystem amber `#f5b54a` → merge periwinkle `#9aa0ff` → studio cyan `#5cc8ff`.
- All motion is gated behind `@media (prefers-reduced-motion: reduce)` → animations off, final static state shown.
- **Definition of done for every task:** `npm run build` completes with **zero warnings**, and the screenshot for that task's section looks correct.

## Verification model (read first)

This is a presentational redesign with no logic surface, so the per-task gate is **build-clean + visual screenshot**, not unit tests. Set up once, reuse everywhere:

- **Dev server (run once, keep running):** from `site/`, `npm run dev` (serves http://localhost:5173 with HMR). On Windows, start it as a background process so it survives across tasks.
- **Screenshot helper (Windows + headless Chrome).** Chrome is a Windows exe and cannot write to MSYS paths, so always write the PNG to a native `C:\…\Temp` path, then read it:

  ```bash
  "/c/Program Files/Google/Chrome/Application/chrome.exe" \
    --headless=new --disable-gpu --no-sandbox --hide-scrollbars \
    --virtual-time-budget=4000 --window-size=1440,1024 \
    --screenshot="C:\\Users\\Kara\\AppData\\Local\\Temp\\shot.png" \
    "http://localhost:5173/"
  # then view C:\Users\Kara\AppData\Local\Temp\shot.png
  ```

  - For mobile checks add `--window-size=390,1600`.
  - For reduced-motion checks add `--force-prefers-reduced-motion` (Chrome flag) and confirm the static end-state renders.
  - Taller `--window-size` height reveals more below-the-fold (IntersectionObserver reveals fire when in view).

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `site/src/app.css` | Modify | Design tokens (`:root`), base typography, page glow, shared classes: `.wrap`, `.btn`, `.eyebrow`, `.section-title`, `.section-lead`, nav |
| `site/src/App.svelte` | Modify | Page composition + data constants only; per-section markup delegates to components |
| `site/src/lib/SeamPanel.svelte` | Create | Animated DISK ⇄ STUDIO seam (hero right visual) |
| `site/src/lib/Hero.svelte` | Create | Split hero composition (left text/CTA + SeamPanel) |
| `site/src/lib/FeatureRow.svelte` | Create | Full-width alternating row: eyebrow, title, lead slot, visual slot, `flip` prop |
| `site/src/lib/BentoGrid.svelte` | Create | Responsive grid container, `cols` prop |
| `site/src/lib/Card.svelte` | Create | Glass card shell, `span` prop, reveal-on-scroll |
| `site/src/lib/DataFlowDiagram.svelte` | Modify | Fit inside FeatureRow column; verify on new tokens |
| `site/src/lib/MergeDiagram.svelte` | Modify | Fit inside FeatureRow column; verify on new tokens |
| `site/src/lib/ArchitectureDiagram.svelte` | Modify | Fit inside FeatureRow column; verify on new tokens |
| `site/src/lib/Icon.svelte` | Keep | Inline-SVG icons |
| `site/src/lib/reveal.js` | Keep | Scroll-reveal action |

Progressive replacement: the site builds and renders at every task boundary. Old section styles in `App.svelte`'s `<style>` are left intact until the task that replaces their markup; Task 8 removes any dead styles.

---

## Task 1: Design tokens + global shell + nav

**Files:**
- Modify: `site/src/app.css` (tokens `:root`, base, page glow, shared `.wrap`/`.btn`/`.eyebrow`/`.section-title`/`.section-lead`/nav)
- Modify: `site/src/App.svelte` (nav markup only)

**Interfaces:**
- Produces (CSS custom properties consumed by every later task): `--bg #070809`, `--bg-soft #0d0f14`, `--bg-card #0d0f14`, `--border #20232c`, `--border-strong #262a34`, `--text #eef0f4`, `--text-dim #9aa0ab`, `--text-faint #6b7280`, `--fs #f5b54a`, `--studio #5cc8ff`, `--merge #9aa0ff`, `--danger #f2616a`, `--ok #4ad295`, `--grad` (the signature gradient), `--radius`, `--radius-sm`, `--maxw 1100px`, `--shadow`, `--dur 0.5s`, `--font`, `--mono`.
- Produces shared classes: `.wrap`, `.btn.primary`, `.btn.ghost`, `.eyebrow`, `.section-title`, `.section-lead`, `.grad`.

- [ ] **Step 1: Replace the `:root` token block and page glow in `app.css`**

Replace lines 1–59 of `site/src/app.css` with:

```css
:root {
  /* Linear-style deep surfaces + tri-color signature accent. */
  --bg: #070809;
  --bg-soft: #0d0f14;
  --bg-card: #0d0f14;
  --border: #20232c;
  --border-strong: #262a34;
  --text: #eef0f4;
  --text-dim: #9aa0ab;
  --text-faint: #6b7280;

  --fs: #f5b54a;       /* filesystem / thread accent (amber) */
  --studio: #5cc8ff;   /* studio accent (cyan) */
  --merge: #9aa0ff;    /* merge accent (periwinkle) */
  --danger: #f2616a;
  --ok: #4ad295;

  --grad: linear-gradient(110deg, var(--fs), var(--merge) 52%, var(--studio));

  --radius: 14px;
  --radius-sm: 9px;
  --maxw: 1100px;
  --shadow: 0 26px 60px -30px rgba(124, 131, 255, 0.55);
  --dur: 0.5s;

  --font: 'Inter', system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
  --mono: 'JetBrains Mono', ui-monospace, 'SF Mono', 'Cascadia Code', Menlo, monospace;
}

* { box-sizing: border-box; }
html { scroll-behavior: smooth; }

body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: var(--font);
  line-height: 1.6;
  -webkit-font-smoothing: antialiased;
  text-rendering: optimizeLegibility;
  overflow-x: hidden;
}

/* Ambient gradient glow behind the page. */
body::before {
  content: '';
  position: fixed;
  inset: 0;
  z-index: -1;
  background:
    radial-gradient(55% 45% at 18% -5%, rgba(245, 181, 74, 0.08), transparent 70%),
    radial-gradient(55% 45% at 85% 8%, rgba(92, 200, 255, 0.08), transparent 70%),
    radial-gradient(50% 50% at 50% 100%, rgba(154, 160, 255, 0.06), transparent 70%);
  pointer-events: none;
}
```

- [ ] **Step 2: Append shared component classes to `app.css`**

After the existing `.wrap` rule (keep it), append:

```css
/* ---- Shared primitives ---- */
.grad {
  background: var(--grad);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
.eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-family: var(--mono);
  font-size: 0.78rem;
  letter-spacing: 0.04em;
  color: var(--text-dim);
  margin-bottom: 16px;
}
.eyebrow :global(svg) { color: var(--fs); }
.section-title {
  font-size: clamp(1.7rem, 3.4vw, 2.5rem);
  letter-spacing: -0.02em;
  line-height: 1.12;
  margin: 0 0 16px;
}
.section-lead {
  max-width: 62ch;
  color: var(--text-dim);
  font-size: 1.05rem;
  margin: 0 0 28px;
}
.section-lead b { color: var(--text); }
.section-lead code, .eyebrow code {
  font-family: var(--mono);
  font-size: 0.86em;
  color: var(--fs);
  background: var(--bg-soft);
  border: 1px solid var(--border);
  border-radius: 5px;
  padding: 1px 5px;
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  padding: 12px 22px;
  border-radius: 11px;
  font-weight: 600;
  font-size: 0.95rem;
  transition: transform 0.15s ease, box-shadow 0.2s ease, border-color 0.2s ease;
}
.btn:hover { transform: translateY(-2px); }
.btn.primary {
  background: linear-gradient(120deg, var(--fs), #f0a030);
  color: #1a1205;
  box-shadow: 0 10px 30px -10px rgba(245, 181, 74, 0.6);
}
.btn.ghost {
  color: var(--text);
  border: 1px solid var(--border-strong);
  background: var(--bg-soft);
}
.btn.ghost:hover { border-color: var(--studio); }

@media (prefers-reduced-motion: reduce) {
  html { scroll-behavior: auto; }
  .btn:hover { transform: none; }
}
```

- [ ] **Step 3: Restyle the nav in `App.svelte`**

In `site/src/App.svelte`, the nav markup (currently lines ~71–87) keeps its structure. Confirm the brand uses the logo `<img>` already in place. In `App.svelte`'s `<style>`, replace the `.brand-mark` rule so the logo renders crisply and the nav uses the new tokens (the nav rules already reference tokens; no change needed beyond `.brand-mark`):

```css
  .brand-mark {
    display: block;
    width: 26px;
    height: 26px;
    image-rendering: pixelated;
  }
```

- [ ] **Step 4: Start the dev server (once) and build**

```bash
cd site
npm run dev      # background, keep running across tasks
npm run build    # must be clean
```
Expected: build succeeds, **zero warnings**.

- [ ] **Step 5: Screenshot the nav + background**

Use the screenshot helper (`--window-size=1440,700`). Expected: darker background (`#070809`), gradient-mark logo + "naht" wordmark, nav links and GitHub pill legible. The old sections below still render (transitional) — that's fine.

- [ ] **Step 6: Commit**

```bash
git add site/src/app.css site/src/App.svelte
git commit -m "redesign: dark Linear tokens, shared primitives, nav"
```

---

## Task 2: Hero + SeamPanel

**Files:**
- Create: `site/src/lib/SeamPanel.svelte`
- Create: `site/src/lib/Hero.svelte`
- Modify: `site/src/App.svelte` (replace `<header class="hero">…</header>` with `<Hero />`; import Hero; remove old hero `<style>` rules)

**Interfaces:**
- Consumes: `Icon`, `reveal`, tokens from Task 1, `logo` from `./assets/logo.png`.
- Produces: `<Hero />` (no props). `<SeamPanel />` (no props).

- [ ] **Step 1: Create `SeamPanel.svelte`**

```svelte
<script>
  // Animated DISK ⇄ STUDIO "seam": a stitched centre line with a file label
  // travelling both ways. Pure CSS/SVG. Motion is disabled under reduced-motion.
</script>

<div class="seam" role="img" aria-label="A file syncing both ways between disk and Roblox Studio">
  <div class="status"><span class="led"></span> connected · re-diffed on connect</div>
  <div class="cols">
    <div class="mini">
      <div class="lab">DISK</div>
      <pre>src/
  Sword.luau
  Hitbox.luau</pre>
    </div>
    <div class="seam-line" aria-hidden="true">
      <span class="stitch"></span>
      <span class="pkt">Sword.luau</span>
    </div>
    <div class="mini">
      <div class="lab">STUDIO</div>
      <pre>ReplicatedStorage
  Sword
  Hitbox</pre>
    </div>
  </div>
  <div class="foot">⇄ synced both ways</div>
</div>

<style>
  .seam {
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--bg-card);
    box-shadow: var(--shadow);
    padding: 18px;
  }
  .status {
    display: flex; align-items: center; gap: 8px;
    font-family: var(--mono); font-size: 0.74rem; color: var(--text-faint);
    margin-bottom: 16px;
  }
  .led { width: 8px; height: 8px; border-radius: 50%; background: var(--ok);
    box-shadow: 0 0 8px var(--ok); }
  .cols { position: relative; display: grid; grid-template-columns: 1fr auto 1fr; gap: 18px; }
  .mini { border: 1px solid var(--border-strong); border-radius: var(--radius-sm);
    background: var(--bg-soft); padding: 12px; }
  .lab { font-family: var(--mono); font-size: 0.66rem; letter-spacing: 0.08em;
    color: var(--text-faint); margin-bottom: 8px; }
  pre { margin: 0; font-family: var(--mono); font-size: 0.78rem; line-height: 1.7;
    color: var(--text-dim); white-space: pre; }
  .seam-line { position: relative; width: 2px; align-self: stretch; }
  .stitch { position: absolute; inset: 0; width: 2px; left: 0;
    background: repeating-linear-gradient(var(--studio) 0 6px, transparent 6px 12px);
    opacity: 0.7; animation: stitch 1.1s linear infinite; }
  .pkt {
    position: absolute; left: 50%; top: 50%;
    transform: translate(-50%, -50%);
    font-family: var(--mono); font-size: 0.6rem; color: #06202b;
    background: var(--studio); border-radius: 999px; padding: 2px 7px; white-space: nowrap;
    animation: travel 3.2s ease-in-out infinite;
  }
  .foot { text-align: center; margin-top: 14px; font-family: var(--mono);
    font-size: 0.72rem; color: var(--studio); }
  @keyframes stitch { to { background-position: 0 12px; } }
  @keyframes travel {
    0%, 100% { transform: translate(-180px, -50%); opacity: 0; }
    15%, 35% { opacity: 1; }
    50% { transform: translate(-50%, -50%); }
    65%, 85% { opacity: 1; }
    50.001% { transform: translate(120px, -50%); }
  }
  @media (max-width: 720px) {
    .cols { grid-template-columns: 1fr; }
    .seam-line { width: auto; height: 2px; margin: 4px 0; }
    .stitch { width: 100%; height: 2px;
      background: repeating-linear-gradient(90deg, var(--studio) 0 6px, transparent 6px 12px); }
  }
  @media (prefers-reduced-motion: reduce) {
    .stitch { animation: none; }
    .pkt { animation: none; transform: translate(-50%, -50%); }
  }
</style>
```

- [ ] **Step 2: Create `Hero.svelte`**

```svelte
<script>
  import Icon from './Icon.svelte'
  import { reveal } from './reveal.js'
  import SeamPanel from './SeamPanel.svelte'

  const REPO = 'https://github.com/vskstudio/naht'
</script>

<header id="top" class="hero">
  <div class="wrap hero-grid">
    <div class="hero-copy">
      <span class="pill reveal" use:reveal>
        <Icon name="spark" size={14} /> Rust · Roblox Studio · bidirectional sync
      </span>
      <h1 class="reveal" use:reveal={{ delay: 60 }}>
        The <span class="grad">seam</span> between your filesystem<br />and Roblox Studio.
      </h1>
      <p class="lead reveal" use:reveal={{ delay: 120 }}>
        Bidirectional sync with a real 3-way merge. Never destructive.
      </p>
      <div class="cta reveal" use:reveal={{ delay: 180 }}>
        <a class="btn primary" href="#start"><Icon name="bolt" size={17} /> Quickstart</a>
        <a class="btn ghost" href="#sync"><Icon name="sync" size={17} /> See it sync</a>
      </div>
    </div>
    <div class="hero-visual reveal" use:reveal={{ delay: 160 }}>
      <SeamPanel />
    </div>
  </div>
</header>

<style>
  .hero { padding: 96px 0 72px; position: relative; }
  .hero-grid {
    display: grid; grid-template-columns: 1fr 1.05fr; gap: 44px; align-items: center;
  }
  .pill {
    display: inline-flex; align-items: center; gap: 8px;
    font-family: var(--mono); font-size: 0.76rem; letter-spacing: 0.04em;
    color: var(--fs); padding: 7px 15px; border: 1px solid var(--border-strong);
    border-radius: 999px; background: var(--bg-soft); margin-bottom: 24px;
  }
  h1 {
    font-size: clamp(2.1rem, 5vw, 3.7rem);
    letter-spacing: -0.02em; line-height: 1.08; margin: 0 0 18px;
  }
  .lead { color: var(--text-dim); font-size: 1.1rem; max-width: 44ch; margin: 0 0 28px; }
  .cta { display: flex; gap: 13px; flex-wrap: wrap; }
  @media (min-width: 721px) { h1 { white-space: nowrap; } }
  @media (max-width: 720px) {
    .hero { padding: 60px 0 48px; text-align: center; }
    .hero-grid { grid-template-columns: 1fr; gap: 32px; }
    .pill, .cta { justify-content: center; }
    .lead { margin-left: auto; margin-right: auto; }
  }
</style>
```

- [ ] **Step 3: Wire `<Hero />` into `App.svelte`**

In `site/src/App.svelte`: add `import Hero from './lib/Hero.svelte'` near the other lib imports. Replace the entire `<header id="top" class="hero">…</header>` block (the current hero, ~lines 89–112 including the terminal mockup) with:

```svelte
<Hero />
```

Then delete the now-unused hero `<style>` rules in `App.svelte` (`.hero`, `.hero-inner`, `.pill`, `.hero h1`, `.grad`, `.hero-cta`, `.hero-term`, `.term-bar`, and the `<pre>` term styles, plus the `@media` hero overrides). Leave nav, section, and other styles. If `Icon`/`reveal` become unused in `App.svelte` after later tasks, remove them then — not now.

- [ ] **Step 4: Build**

```bash
cd site && npm run build
```
Expected: clean, zero warnings (watch for "unused CSS selector" — remove any leftover hero rules it flags).

- [ ] **Step 5: Screenshot hero — desktop and mobile**

Desktop `--window-size=1440,1024`: split layout, headline on exactly **two lines**, SeamPanel on the right with the stitched centre line and travelling `Sword.luau` packet, amber Quickstart + ghost See-it-sync.
Mobile `--window-size=390,1600`: single stacked column, centered, SeamPanel below the copy, headline wraps gracefully.

- [ ] **Step 6: Commit**

```bash
git add site/src/lib/SeamPanel.svelte site/src/lib/Hero.svelte site/src/App.svelte
git commit -m "redesign: split hero with animated seam panel"
```

---

## Task 3: FeatureRow + Sync row

**Files:**
- Create: `site/src/lib/FeatureRow.svelte`
- Modify: `site/src/lib/DataFlowDiagram.svelte` (fit width)
- Modify: `site/src/App.svelte` (add the Sync `<FeatureRow>` using `DataFlowDiagram`)

**Interfaces:**
- Consumes: `Icon`, `reveal`, tokens.
- Produces: `<FeatureRow eyebrow={string} icon={string} title={string} flip={boolean} >` with a `lead` named slot (rich text paragraph) and a default slot (the visual). `flip` (default `false`) swaps text/visual sides on desktop.

- [ ] **Step 1: Create `FeatureRow.svelte`**

```svelte
<script>
  import Icon from './Icon.svelte'
  import { reveal } from './reveal.js'
  export let eyebrow = ''
  export let icon = ''
  export let title = ''
  export let flip = false
</script>

<section class="row" class:flip>
  <div class="copy">
    {#if eyebrow}
      <div class="eyebrow reveal" use:reveal>
        {#if icon}<Icon name={icon} size={14} />{/if} {eyebrow}
      </div>
    {/if}
    <h2 class="section-title reveal" use:reveal>{title}</h2>
    <div class="section-lead reveal" use:reveal><slot name="lead" /></div>
  </div>
  <div class="visual reveal" use:reveal={{ delay: 120 }}>
    <slot />
  </div>
</section>

<style>
  .row {
    display: grid; grid-template-columns: 1fr 1.05fr; gap: 44px; align-items: center;
    max-width: var(--maxw); margin: 0 auto; padding: 72px 24px;
  }
  .row.flip .copy { order: 2; }
  .row.flip .visual { order: 1; }
  .visual { min-width: 0; }
  @media (max-width: 720px) {
    .row, .row.flip { grid-template-columns: 1fr; gap: 28px; padding: 52px 24px; }
    .row.flip .copy, .row.flip .visual { order: 0; }
  }
</style>
```

- [ ] **Step 2: Make `DataFlowDiagram` fill its column**

Open `site/src/lib/DataFlowDiagram.svelte`. In its scoped `<style>`, ensure the root wrapper is fluid: set the outermost container to `width: 100%; max-width: 560px; margin-inline: auto;` (add it to the existing root rule rather than a fixed pixel width). Do not change its logic, labels, or token references.

- [ ] **Step 3: Replace the Sync section markup in `App.svelte`**

Add `import FeatureRow from './lib/FeatureRow.svelte'`. Replace the current `<section id="sync">…</section>` (lines ~149–168) — only the DataFlow part for now (the Merge `<h3>`/lead/`MergeDiagram` move to Task 4) — with:

```svelte
<FeatureRow id="sync" eyebrow="The seam, working" icon="sync"
  title="Bidirectional sync with a persisted base.">
  <p slot="lead">
    The filesystem is read fresh on every reconcile; the Studio side is mirrored in memory.
    Filesystem → Studio patches are <b>ack-gated</b> — the base advances only once the plugin
    confirms it applied, so a half-applied batch re-diffs the rest instead of clobbering.
  </p>
  <DataFlowDiagram />
</FeatureRow>
```

Note: `FeatureRow` needs to accept an `id`. Add `export let id = undefined` to its script and put `id={id}` on the `<section>` so in-page anchors (`#sync`) still work.

- [ ] **Step 4: Build**

```bash
cd site && npm run build
```
Expected: clean. (The Merge diagram is temporarily orphaned in the old section block until Task 4 — keep the rest of the old `#sync` section removed and the `MergeDiagram` import intact so the build has no unused-import error; if Svelte flags the unused import, leave a temporary `<!-- MergeDiagram used in Task 4 -->` is NOT valid — instead complete Task 4 immediately after, or comment the import line until Task 4.)

- [ ] **Step 5: Screenshot the Sync row**

`--window-size=1440,1400`. Expected: left copy (eyebrow, title, lead with bold "ack-gated"), right DataFlowDiagram, comfortable alignment.

- [ ] **Step 6: Commit**

```bash
git add site/src/lib/FeatureRow.svelte site/src/lib/DataFlowDiagram.svelte site/src/App.svelte
git commit -m "redesign: FeatureRow + sync feature row"
```

---

## Task 4: Merge row + Architecture row

**Files:**
- Modify: `site/src/lib/MergeDiagram.svelte` (fit width)
- Modify: `site/src/lib/ArchitectureDiagram.svelte` (fit width)
- Modify: `site/src/App.svelte` (add Merge row `flip`, Architecture row; remove old `#architecture` section)

**Interfaces:**
- Consumes: `FeatureRow` (Task 3), `MergeDiagram`, `ArchitectureDiagram`.
- Produces: two more `<FeatureRow>` sections in the page flow.

- [ ] **Step 1: Fit both diagrams to their column**

In `MergeDiagram.svelte` and `ArchitectureDiagram.svelte`, set the outermost wrapper to `width: 100%; max-width: 560px; margin-inline: auto;` in the scoped `<style>` (same as Task 3 Step 2). No logic/label/token changes.

- [ ] **Step 2: Add the Merge row (flipped) in `App.svelte`**

Immediately after the Sync `<FeatureRow>`, add (re-using the existing merge copy):

```svelte
<FeatureRow eyebrow="When both sides change the same script" icon="merge"
  title="A real 3-way merge against the last-sync base." flip>
  <p slot="lead">
    Clean merges are written and the base advances; an unmergeable hunk freezes the path with
    git-style markers and never auto-picks a winner.
  </p>
  <MergeDiagram />
</FeatureRow>
```

If the `MergeDiagram` import was commented at the end of Task 3, uncomment it now.

- [ ] **Step 3: Replace the Architecture section in `App.svelte`**

Remove the old `<section id="architecture" class="alt">…</section>` (lines ~136–147) and add an Architecture `<FeatureRow>` in its new position (after the Merge row, per the spec's order):

```svelte
<FeatureRow id="architecture" eyebrow="Architecture" icon="layers"
  title="A Cargo workspace and a thin Luau plugin.">
  <p slot="lead">
    The brain has zero network I/O so it stays unit-testable; the daemon owns transport and the
    disk; the plugin is kept deliberately thin — the thinner the plugin, the fewer the bugs.
  </p>
  <ArchitectureDiagram />
</FeatureRow>
```

- [ ] **Step 4: Build**

```bash
cd site && npm run build
```
Expected: clean, zero warnings.

- [ ] **Step 5: Screenshot Merge + Architecture rows**

`--window-size=1440,2200`. Expected: Merge row with visual on the **left** (flipped), Architecture row with visual on the right, both diagrams readable on the dark surface.

- [ ] **Step 6: Commit**

```bash
git add site/src/lib/MergeDiagram.svelte site/src/lib/ArchitectureDiagram.svelte site/src/App.svelte
git commit -m "redesign: merge + architecture feature rows"
```

---

## Task 5: BentoGrid + Card + Failure-modes section

**Files:**
- Create: `site/src/lib/BentoGrid.svelte`
- Create: `site/src/lib/Card.svelte`
- Modify: `site/src/App.svelte` (replace `#why` section markup using `BentoGrid`/`Card`; keep the `pains` constant)

**Interfaces:**
- Consumes: `reveal`, tokens.
- Produces:
  - `<BentoGrid cols={number} >` (default `cols=3`) — responsive grid, default slot holds cards.
  - `<Card span={number} reveal-delay implicit>` (default `span=1`) — glass card shell with hover lift; default slot holds card content. Card applies `use:reveal` itself.

- [ ] **Step 1: Create `BentoGrid.svelte`**

```svelte
<script>
  export let cols = 3
</script>

<div class="bento" style="--cols:{cols}">
  <slot />
</div>

<style>
  .bento {
    display: grid;
    grid-template-columns: repeat(var(--cols), 1fr);
    gap: 16px;
  }
  @media (max-width: 860px) { .bento { grid-template-columns: repeat(2, 1fr); } }
  @media (max-width: 560px) { .bento { grid-template-columns: 1fr; } }
</style>
```

- [ ] **Step 2: Create `Card.svelte`**

```svelte
<script>
  import { reveal } from './reveal.js'
  export let span = 1
  export let delay = 0
</script>

<div class="card reveal" style="grid-column: span {span}" use:reveal={{ delay }}>
  <slot />
</div>

<style>
  .card {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-card);
    padding: 18px;
    transition: transform 0.15s ease, border-color 0.2s ease, box-shadow 0.2s ease;
  }
  .card:hover {
    transform: translateY(-3px);
    border-color: var(--border-strong);
    box-shadow: 0 18px 40px -24px rgba(124, 131, 255, 0.5);
  }
  @media (max-width: 560px) { .card { grid-column: span 1 !important; } }
  @media (prefers-reduced-motion: reduce) { .card:hover { transform: none; } }
</style>
```

- [ ] **Step 3: Replace the `#why` (failure-modes) section in `App.svelte`**

Add imports `BentoGrid`, `Card`. Replace the `.pain-grid` block inside `<section id="why">` with `BentoGrid`/`Card`, keeping the eyebrow/title/lead and the `pains` constant:

```svelte
<section id="why">
  <div class="wrap">
    <div class="eyebrow reveal" use:reveal><Icon name="warn" size={14} /> Why Naht</div>
    <h2 class="section-title reveal" use:reveal>Built around the failure modes that make the others painful.</h2>
    <p class="section-lead reveal" use:reveal>
      A from-scratch alternative to Rojo and Argon. Every decision is grounded in a concrete failure
      of the incumbents — see the prior-art teardown in the repo.
    </p>
    <BentoGrid cols={3}>
      {#each pains as p, i (p.pain)}
        <Card delay={(i % 3) * 90}>
          <span class="pain-icon"><Icon name={p.icon} size={20} /></span>
          <div class="pain-bad"><Icon name="warn" size={14} /> {p.pain}</div>
          <div class="pain-fix"><Icon name="check" size={14} /> {p.fix}</div>
        </Card>
      {/each}
    </BentoGrid>
  </div>
</section>
```

Keep the `.pain-icon`, `.pain-bad`, `.pain-fix` rules in `App.svelte`'s `<style>` (they style card *contents*, not the old grid). Remove the now-unused `.pain-grid` and `.pain-card` rules.

- [ ] **Step 4: Build**

```bash
cd site && npm run build
```
Expected: clean. Remove any "unused CSS selector" the build flags (old `.pain-grid`/`.pain-card`).

- [ ] **Step 5: Screenshot the failure-modes bento**

`--window-size=1440,1300`. Expected: 3-column grid of six glass cards, each with icon, muted pain line, bright fix line; hover lift visible on a second screenshot if you emulate hover (optional).

- [ ] **Step 6: Commit**

```bash
git add site/src/lib/BentoGrid.svelte site/src/lib/Card.svelte site/src/App.svelte
git commit -m "redesign: bento grid + cards for failure modes"
```

---

## Task 6: CLI bento + Limits table

**Files:**
- Modify: `site/src/App.svelte` (`#cli` via `BentoGrid`/`Card`; restyle `#limits` table)

**Interfaces:**
- Consumes: `BentoGrid`, `Card` (Task 5), `commands` and `limits` constants.
- Produces: restyled `#cli` and `#limits` sections.

- [ ] **Step 1: Replace the `#cli` grid in `App.svelte`**

Keep the `class="alt"` section wrapper, eyebrow/title/lead and the `commands` constant. Replace `.cmd-grid` with:

```svelte
<BentoGrid cols={2}>
  {#each commands as c, i (c.cmd)}
    <Card delay={(i % 2) * 80}>
      <code class="cmd"><span class="prompt">$</span> {c.cmd}</code>
      <p class="cmd-desc">{c.desc}</p>
    </Card>
  {/each}
</BentoGrid>
```

In `App.svelte`'s `<style>`, replace the old `.cmd-card`/`.cmd-grid` rules with content styles:

```css
  .cmd { font-family: var(--mono); font-size: 0.9rem; color: var(--text); display: block; }
  .cmd .prompt { color: var(--fs); margin-right: 6px; }
  .cmd-desc { color: var(--text-dim); font-size: 0.9rem; margin: 8px 0 0; }
```

- [ ] **Step 2: Restyle the `#limits` table**

Keep the markup (`.limits`, `.limit-row`, `.limit-case`, `.limit-status`, `.status`, `.limit-note`) and the `limits` constant. Update its `<style>` rules to the new tokens — replace the `.limit-row` border/background with `var(--border)`/`var(--bg-card)`, the `.status.ok/.warn/.bad` chips to use `--ok`/`--fs`/`--danger` with low-opacity fills:

```css
  .limits { border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  .limit-row {
    display: grid; grid-template-columns: 1fr auto 2fr; gap: 16px; align-items: center;
    padding: 16px 18px; border-top: 1px solid var(--border); background: var(--bg-card);
  }
  .limit-row:first-child { border-top: none; }
  .limit-case { font-family: var(--mono); font-size: 0.9rem; color: var(--text); }
  .status { font-family: var(--mono); font-size: 0.72rem; padding: 3px 10px; border-radius: 999px; }
  .status.ok { color: var(--ok); background: rgba(74, 210, 149, 0.12); }
  .status.warn { color: var(--fs); background: rgba(245, 181, 74, 0.12); }
  .status.bad { color: var(--danger); background: rgba(242, 97, 106, 0.12); }
  .limit-note { color: var(--text-dim); font-size: 0.9rem; }
  @media (max-width: 640px) { .limit-row { grid-template-columns: 1fr; gap: 6px; } }
```

- [ ] **Step 3: Build**

```bash
cd site && npm run build
```
Expected: clean; remove any flagged unused selectors.

- [ ] **Step 4: Screenshot CLI + Limits**

`--window-size=1440,1700`. Expected: 2-column command cards (mono command, `$` in amber, muted description); limits table with colored status chips (upload/syncable green, round-trip amber, hard block red).

- [ ] **Step 5: Commit**

```bash
git add site/src/App.svelte
git commit -m "redesign: CLI bento + limits table on new tokens"
```

---

## Task 7: Quickstart frieze + Footer

**Files:**
- Modify: `site/src/App.svelte` (`#start` steps + footer)

**Interfaces:**
- Consumes: `steps` constant, `Icon`, `reveal`, `logo`, `btn` classes.
- Produces: restyled quickstart + footer.

- [ ] **Step 1: Restyle the `#start` steps**

Keep the section (`class="alt"`), eyebrow/title/lead, the CTA buttons, and the `steps` constant + markup. Update the `.steps`/`.step`/`.step-n` rules to the new tokens:

```css
  .steps { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-bottom: 28px; }
  .step {
    display: flex; gap: 13px; padding: 18px; border: 1px solid var(--border);
    border-radius: var(--radius); background: var(--bg-card);
  }
  .step-n {
    flex: none; width: 28px; height: 28px; border-radius: 50%;
    display: grid; place-items: center; font-family: var(--mono); font-size: 0.85rem;
    color: #1a1205; background: linear-gradient(120deg, var(--fs), #f0a030);
  }
  .step strong { display: block; margin-bottom: 4px; }
  .step p { margin: 0; color: var(--text-dim); font-size: 0.9rem; }
  .start-cta { display: flex; gap: 13px; flex-wrap: wrap; }
  @media (max-width: 860px) { .steps { grid-template-columns: 1fr; } }
```

- [ ] **Step 2: Restyle the footer**

Keep the footer markup (brand `<img>` logo + links + license). Update `.foot-*` rules to new tokens; ensure `.brand-mark` in the footer renders the logo at ~30px with `image-rendering: pixelated`. Confirm the footer brand uses the same `logo` import already present.

```css
  footer { border-top: 1px solid var(--border); padding: 40px 0; margin-top: 40px; }
  .foot-inner { display: flex; flex-wrap: wrap; gap: 24px; align-items: center; justify-content: space-between; }
  .foot-brand { display: flex; align-items: center; gap: 12px; }
  .foot-brand span { color: var(--text-dim); font-size: 0.85rem; }
  .foot-links { display: flex; gap: 18px; }
  .foot-links a { color: var(--text-dim); font-size: 0.9rem; }
  .foot-links a:hover { color: var(--text); }
  .foot-license { color: var(--text-faint); font-size: 0.82rem; }
```

- [ ] **Step 3: Build**

```bash
cd site && npm run build
```
Expected: clean.

- [ ] **Step 4: Screenshot quickstart + footer**

`--window-size=1440,1400`. Expected: 3-column step frieze with gradient numbered badges, two CTA buttons, footer with logo + links + license.

- [ ] **Step 5: Commit**

```bash
git add site/src/App.svelte
git commit -m "redesign: quickstart frieze + footer on new tokens"
```

---

## Task 8: Motion, reduced-motion, responsive & a11y final sweep

**Files:**
- Modify: `site/src/App.svelte` (remove dead styles; final polish)
- Modify: any component needing a reduced-motion guard or responsive fix found during the sweep

**Interfaces:**
- Consumes: everything above. Produces: the finished, coherent page.

- [ ] **Step 1: Remove dead CSS and unused imports in `App.svelte`**

Search `App.svelte`'s `<style>` for selectors no longer present in markup (old `.hero*`, `.term-*`, `.pain-grid`, `.pain-card`, `.cmd-grid`, `.cmd-card`, `.sub`, etc.) and delete them. Remove any now-unused script imports (e.g. `DataFlowDiagram`/`MergeDiagram`/`ArchitectureDiagram` are still used; verify each import is referenced). Build must stay clean.

- [ ] **Step 2: Verify reduced-motion across the page**

Take a full-page screenshot with `--force-prefers-reduced-motion --window-size=1440,6800`. Expected: SeamPanel packet sits centered/static, no stitch animation, reveal elements are visible (not stuck at opacity 0), no hover transforms. If any element is invisible under reduced motion, ensure `reveal.js`'s reduced-motion path (or a `@media (prefers-reduced-motion: reduce)` rule) shows the final state. Add a guard where missing:

```css
@media (prefers-reduced-motion: reduce) {
  .reveal { opacity: 1 !important; transform: none !important; }
}
```
(Place in `app.css` if `reveal.js` relies on a `.reveal` class for its initial hidden state.)

- [ ] **Step 3: Responsive sweep at mobile width**

Full-page mobile screenshot `--window-size=390,7000`. Expected: hero stacks and centers; every FeatureRow stacks (visual under copy) with no horizontal overflow; bento grids collapse to 1 column ≤560px; limits rows stack; nav non-CTA links hide per the existing `@media (max-width: 720px)` nav rule. Fix any overflow (usually a diagram or `<pre>` needing `max-width:100%` / `overflow-x:auto`).

- [ ] **Step 4: Full desktop coherence pass**

Full-page desktop screenshot `--window-size=1440,6800`. Confirm consistent rhythm, the gradient is used sparingly (seam word, accents), spacing is even between sections, and the tri-color reads as one system. Tweak section paddings if uneven.

- [ ] **Step 5: Final clean build**

```bash
cd site && npm run build
```
Expected: **zero warnings**, reasonable bundle (no unexpected size jump vs. before).

- [ ] **Step 6: Commit**

```bash
git add site/src
git commit -m "redesign: reduced-motion, responsive & a11y final sweep"
```

---

## Self-Review

**Spec coverage:**
- Stack/scope kept (Svelte+Vite, content carried over) → Global Constraints + every task. ✔
- Linear dark system + tri-color signature gradient → Task 1 tokens. ✔
- Split hero with seam visual + short subline → Task 2. ✔
- Hybrid body: alternating rows (Sync/Merge/Architecture) → Tasks 3–4; bento (failure modes/CLI) + limits table → Tasks 5–6. ✔
- Quickstart frieze + footer → Task 7. ✔
- Component breakdown (Hero, SeamPanel, FeatureRow, BentoGrid, Card; diagrams restyled; Icon/reveal kept) → File Structure + Tasks. ✔
- Motion gated by prefers-reduced-motion + AA contrast + responsive → Task 8 (and per-component guards). ✔
- Build clean + screenshot verification, `base:'./'` kept → Verification model + Global Constraints. ✔

**Placeholder scan:** No TBD/TODO; each code step shows complete code; diagram restyles give the exact CSS rule to add. ✔

**Type/name consistency:** `FeatureRow` props `eyebrow/icon/title/flip/id` used identically in Tasks 3–4; `Card` props `span/delay` and `BentoGrid` prop `cols` used identically in Tasks 5–6; token names defined in Task 1 are the ones referenced throughout. ✔

One transitional caveat is called out explicitly (Task 3 ↔ 4 `MergeDiagram` import) so the build stays green between commits.
