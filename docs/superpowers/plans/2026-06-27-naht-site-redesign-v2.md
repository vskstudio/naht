# Naht site redesign v2 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the Naht landing page in `site/` to match the approved mockup `fullpage-v8.html` (7 focused blocks, each with a distinct "real artifact" visual) and add bespoke FR/EN internationalization with a language toggle.

**Architecture:** A single Svelte 5 page. All user-visible prose lives in two dictionaries (`src/i18n/en.js`, `src/i18n/fr.js`) exposed through a `locale` writable store and a derived `t` store; components read `$t.<section>.<key>`. Each section's visual is a small presentational component translated from the mockup's HTML/CSS into a scoped Svelte component. Layout, palette, and the gradient are unchanged from the existing site.

**Tech Stack:** Svelte 5 (runes: `$props`, `$derived`, `$state`), Vite, plain CSS (scoped + the existing `app.css` tokens). Vitest + jsdom for the i18n logic and dictionary-parity tests.

**Testing strategy (read before starting):** The i18n module is real logic (locale detection, fallback, persistence, dictionary key-parity) and is built test-first with Vitest. The visual/artifact components are presentational — they have no branching logic, so their gate is `npm run build` succeeding plus the final manual QA checklist (Task 16), not per-component unit tests. Do not invent low-value render tests for static markup.

**Visual source of truth:** `.superpowers/brainstorm/33435-1782525587/content/fullpage-v8.html`. When this plan and the mockup disagree on a pixel, the mockup wins — open it for exact spacing/colors. The standalone artifact mockups (`merge-editor.html`, `base-reconnect.html`, `arch-try.html`) carry the same markup the component tasks reproduce.

**Palette tokens (already in `src/app.css`):** `--bg #070809`, `--bg-soft/--bg-card #0d0f14`, `--border #20232c`, `--border-strong #262a34`, `--border-soft #15171d`, `--text #eef0f4`, `--text-dim #9aa0ab`, `--text-faint #8c93a0`, `--fs #f5b54a`, `--studio #5cc8ff`, `--merge #9aa0ff`, `--ok #4ad295`, `--danger #f2616a`, `--grad`, `--mono`, `--font`. **Task 1 widens `--maxw` from 1100px to 1440px.**

---

## File structure

**Created:**
- `site/vitest.config.js` — test runner config (jsdom env).
- `site/src/i18n/index.js` — `locale` store, `setLocale`, `detectLocale`, derived `t`, `LOCALES`.
- `site/src/i18n/en.js` — English dictionary.
- `site/src/i18n/fr.js` — French dictionary (mockup copy is the source).
- `site/src/i18n/index.test.js` — i18n logic tests.
- `site/src/i18n/dictionaries.test.js` — en/fr key-parity test.
- `site/src/lib/LanguageToggle.svelte` — FR/EN switch in the nav.
- `site/src/lib/ComparisonMatrix.svelte` — block 01.
- `site/src/lib/MergeEditor.svelte` — block 02 artifact.
- `site/src/lib/StateInspector.svelte` — block 03 artifact.
- `site/src/lib/LogStream.svelte` — block 04 artifact.
- `site/src/lib/WorkspaceTree.svelte` — block 05 artifact.
- `site/src/lib/GuidedTerminal.svelte` — block 06 artifact.

**Modified:**
- `site/package.json` — add devDeps + test scripts.
- `site/src/app.css` — `--maxw: 1440px`.
- `site/src/lib/SeamPanel.svelte` — replace with the approved floating-seam hero visual.
- `site/src/lib/Hero.svelte` — copy via `$t`, keep CTA buttons.
- `site/src/lib/FeatureRow.svelte` — accept `eyebrowNum`, keep slot API.
- `site/src/App.svelte` — full rebuild: nav + toggle, 7 blocks, footer, all via `$t`.

**Deleted (after App.svelte no longer imports them):**
- `site/src/lib/DataFlowDiagram.svelte`, `site/src/lib/MergeDiagram.svelte`, `site/src/lib/ArchitectureDiagram.svelte`, `site/src/lib/BentoGrid.svelte`.

---

## Task 1: Tooling + maxw token

**Files:**
- Modify: `site/package.json`
- Modify: `site/src/app.css:25`
- Create: `site/vitest.config.js`

- [ ] **Step 1: Add dev dependencies and test scripts**

Edit `site/package.json`. Add to `scripts`:

```json
"test": "vitest run",
"test:watch": "vitest"
```

Add to `devDependencies` (keep existing entries):

```json
"vitest": "^2.1.8",
"jsdom": "^25.0.1"
```

- [ ] **Step 2: Install**

Run: `cd site && npm install`
Expected: completes, `node_modules/.bin/vitest` exists.

- [ ] **Step 3: Create vitest config**

Create `site/vitest.config.js`:

```js
import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.js'],
  },
})
```

- [ ] **Step 4: Widen the page**

In `site/src/app.css`, change line 25 from `--maxw: 1100px;` to:

```css
  --maxw: 1440px;
```

- [ ] **Step 5: Sanity check the runner**

Run: `cd site && npx vitest run`
Expected: exits 0 with "No test files found" (no tests yet).

- [ ] **Step 6: Commit**

```bash
git add site/package.json site/package-lock.json site/vitest.config.js site/src/app.css
git commit -m "chore(site): add vitest tooling, widen maxw to 1440"
```

---

## Task 2: i18n core (test-first)

**Files:**
- Create: `site/src/i18n/index.js`
- Test: `site/src/i18n/index.test.js`

The store has import-time side effects (persist + `<html lang>`), so the test imports it dynamically after seeding `localStorage`.

- [ ] **Step 1: Write the failing test**

Create `site/src/i18n/index.test.js`:

```js
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { get } from 'svelte/store'

beforeEach(() => {
  localStorage.clear()
  vi.resetModules()
})

describe('detectLocale', () => {
  it('prefers a valid saved locale', async () => {
    localStorage.setItem('naht_locale', 'fr')
    const { detectLocale } = await import('./index.js')
    expect(detectLocale()).toBe('fr')
  })

  it('ignores an invalid saved locale and falls back to en', async () => {
    localStorage.setItem('naht_locale', 'zz')
    const { detectLocale } = await import('./index.js')
    expect(detectLocale()).toBe('en')
  })

  it('uses navigator.language prefix when nothing is saved', async () => {
    vi.spyOn(navigator, 'language', 'get').mockReturnValue('fr-FR')
    const { detectLocale } = await import('./index.js')
    expect(detectLocale()).toBe('fr')
  })
})

describe('setLocale', () => {
  it('updates the store and persists', async () => {
    const { locale, setLocale } = await import('./index.js')
    setLocale('fr')
    expect(get(locale)).toBe('fr')
    expect(localStorage.getItem('naht_locale')).toBe('fr')
    expect(document.documentElement.lang).toBe('fr')
  })

  it('rejects unknown codes', async () => {
    const { locale, setLocale } = await import('./index.js')
    const before = get(locale)
    setLocale('zz')
    expect(get(locale)).toBe(before)
  })
})

describe('t', () => {
  it('exposes the dictionary for the active locale', async () => {
    const { t, setLocale } = await import('./index.js')
    setLocale('en')
    expect(get(t).hero.ctaPrimary).toBeTypeOf('string')
    setLocale('fr')
    expect(get(t).hero.ctaPrimary).toBeTypeOf('string')
  })
})
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cd site && npx vitest run src/i18n/index.test.js`
Expected: FAIL — cannot resolve `./index.js` (and `./en.js`/`./fr.js`).

- [ ] **Step 3: Create the i18n core**

Create `site/src/i18n/index.js`:

```js
import { writable, derived } from 'svelte/store'
import en from './en.js'
import fr from './fr.js'

const dicts = { en, fr }
export const LOCALES = ['en', 'fr']
const STORAGE_KEY = 'naht_locale'

export function detectLocale() {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved && LOCALES.includes(saved)) return saved
  }
  if (typeof navigator !== 'undefined' && navigator.language) {
    const prefix = navigator.language.slice(0, 2).toLowerCase()
    if (LOCALES.includes(prefix)) return prefix
  }
  return 'en'
}

export const locale = writable(detectLocale())

export function setLocale(code) {
  if (LOCALES.includes(code)) locale.set(code)
}

locale.subscribe((code) => {
  if (typeof localStorage !== 'undefined') localStorage.setItem(STORAGE_KEY, code)
  if (typeof document !== 'undefined') document.documentElement.lang = code
})

export const t = derived(locale, ($locale) => dicts[$locale])
```

Note: `en.js`/`fr.js` are created in Task 3; this test stays red until then. That is expected — proceed to Task 3, then re-run.

- [ ] **Step 4: Commit (test + core, still red pending dicts)**

```bash
git add site/src/i18n/index.js site/src/i18n/index.test.js
git commit -m "feat(site): i18n core store with locale detection (tests pending dicts)"
```

---

## Task 3: Dictionaries (en + fr) + parity test

**Files:**
- Create: `site/src/i18n/en.js`
- Create: `site/src/i18n/fr.js`
- Test: `site/src/i18n/dictionaries.test.js`

- [ ] **Step 1: Write the parity test first**

Create `site/src/i18n/dictionaries.test.js`:

```js
import { describe, it, expect } from 'vitest'
import en from './en.js'
import fr from './fr.js'

function leafPaths(obj, prefix = '') {
  const out = []
  for (const [k, v] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${k}` : k
    if (Array.isArray(v)) {
      out.push(`${path}[]:${v.length}`)
      v.forEach((item, i) => {
        if (item && typeof item === 'object') out.push(...leafPaths(item, `${path}[${i}]`))
      })
    } else if (v && typeof v === 'object') {
      out.push(...leafPaths(v, path))
    } else {
      out.push(path)
    }
  }
  return out.sort()
}

describe('dictionaries', () => {
  it('en and fr have identical key shape and array lengths', () => {
    expect(leafPaths(fr)).toEqual(leafPaths(en))
  })

  it('no value is an empty string', () => {
    const check = (obj) => {
      for (const v of Object.values(obj)) {
        if (typeof v === 'string') expect(v.length).toBeGreaterThan(0)
        else if (v && typeof v === 'object') check(v)
      }
    }
    check(en)
    check(fr)
  })
})
```

- [ ] **Step 2: Run to confirm failure**

Run: `cd site && npx vitest run src/i18n/dictionaries.test.js`
Expected: FAIL — cannot resolve `./en.js`.

- [ ] **Step 3: Create the English dictionary**

Create `site/src/i18n/en.js`:

```js
export default {
  nav: {
    comparison: 'Comparison',
    merge: 'Merge',
    architecture: 'Architecture',
    quickstart: 'Quickstart',
    github: 'GitHub',
  },
  toggle: { aria: 'Language' },

  hero: {
    pill: 'Rust · Roblox Studio · bidirectional',
    title1: 'The ',
    titleAccent: 'seam',
    title2: ' between your disk and Roblox Studio.',
    sub: 'Bidirectional sync with a real 3-way merge against a persisted base. Never destructive — a failed write pauses one lane, it never kills the session.',
    ctaPrimary: 'Quickstart',
    ctaSecondary: 'See it sync',
    meta: ['0 unwrap() in the loop', '3-way text merge', 'SQLite persisted base'],
    seam: {
      fsHeader: 'Filesystem',
      studioHeader: 'Studio Explorer',
      status: 'Connected · base ack-gated · re-diff on reconnect',
    },
  },

  matrix: {
    num: '01',
    label: 'Comparison',
    title: 'Built around the failure modes that make the others painful.',
    lead: 'A from-scratch alternative to Rojo and Argon. Every row is anchored in a concrete failure of the incumbents — detailed just below.',
    note: 'Source: docs/prior-art.md — sourced teardown (June 2026).',
    head: { naht: 'Naht', rojo: 'Rojo', argon: 'Argon' },
    rows: [
      { label: 'Bidirectional sync', naht: 'core', rojo: 'experimental', argon: '✓', rojoCls: 'wn', argonCls: 'yes' },
      { label: '3-way text merge', naht: '✓', rojo: '✗ overwrite', argon: '✗ instance', rojoCls: 'no', argonCls: 'no' },
      { label: 'Persisted last-sync base', naht: 'SQLite', rojo: '✗ memory', argon: '✗ memory', rojoCls: 'no', argonCls: 'no' },
      { label: 'Reconnect + visible state', naht: '✓', rojo: '✗', argon: 'partial', rojoCls: 'no', argonCls: 'wn' },
      { label: 'Non-destructive (zero unwrap)', naht: '✓', rojo: '✗ crash', argon: '~', rojoCls: 'no', argonCls: 'wn' },
      { label: 'Config by convention + layered', naht: '✓', rojo: '✗ verbose', argon: '~', rojoCls: 'no', argonCls: 'wn' },
      { label: 'Non-syncable props surfaced', naht: '✓', rojo: '✗ silent', argon: '✗', rojoCls: 'no', argonCls: 'no' },
    ],
  },

  merge: {
    num: '02',
    tag: 'differentiator #1',
    title: 'A real 3-way merge, never an arbitrary winner.',
    lead: 'Disk and Studio are merged against the last-sync base. A clean hunk is written and the base advances; a conflicting hunk freezes the path with git markers — Naht never picks for you.',
    leadVs: 'Rojo: overwrite-on-conflict. Argon: instance-level resolution, not text.',
    tab: 'PlayerData.lua — Merging (1 conflict)',
    diskHeader: 'DISK',
    diskSub: 'current · up to date',
    studioHeader: 'STUDIO',
    studioSub: 'incoming · edited',
    accept: 'Accept ▸',
    resultHeader: 'RESULT',
    baseTag: 'base last-sync · SQLite',
    conflictBar: 'Conflict 1 — m.speed',
    actionDisk: 'Accept Disk',
    actionStudio: 'Accept Studio',
    actionBoth: 'Both',
    autoMerged: '-- ✓ auto-merged',
    footerClean: '1 clean hunk → base advances',
    footerConflict: '1 conflict → path frozen, never auto-resolved',
  },

  base: {
    num: '03',
    tag: 'survives restarts',
    title: 'Reconciliation state lives on disk, not in RAM.',
    lead: 'The last-sync base is in SQLite. A crash, a daemon restart, a network drop — on return Naht re-diffs against the persisted state instead of re-clobbering. Filesystem → Studio patches are ack-gated: the base advances only once the plugin confirms.',
    leadVs: 'Rojo & Argon: sync state in memory, lost on restart.',
    inspectorTitle: 'naht — state inspector',
    query: 'SELECT * FROM base ORDER BY synced_at DESC',
    cols: { path: 'path', hash: 'blob_hash', rev: 'rev', ts: 'synced_at' },
    footer: 'daemon restarted · base reloaded from disk · re-diff 142 paths → 0 re-clobber',
  },

  nondestructive: {
    num: '04',
    tag: 'zero unwrap() in the loop',
    title: 'A failure pauses one lane. It never kills the session.',
    lead: 'When a write fails, the affected lane goes to pause — the other direction keeps going. Heartbeat, auto-reconnect with backoff, always-visible state. A dying connection is neither silent nor fatal.',
    leadVs: 'Rojo: the two-way can crash the server (.unwrap()). Argon: sync cut when the widget closes.',
    termTitle: 'zsh — naht serve -vv',
    logs: [
      { time: '12:04:30', lv: 'info', msg: 'watching <b>src/</b> · plugin <span class="em-ok">connected</span>' },
      { time: '12:04:31', lv: 'err', msg: 'write failed <span class="lane">FS→Studio</span> <b>PlayerData</b> — <span class="em-err">DataModel busy</span>' },
      { time: '12:04:31', lv: 'warn', msg: 'lane <span class="lane">FS→Studio</span> <span class="em-warn">paused</span> · session intact, no unwrap' },
      { time: '12:04:31', lv: 'info', msg: 'lane <span class="lane">Studio→FS</span> keeps going uninterrupted' },
      { time: '12:04:46', lv: 'warn', msg: 'connection lost · heartbeat timeout' },
      { time: '12:04:47', lv: 'info', msg: 'reconnect backoff <b>1s → 2s → 4s</b>' },
      { time: '12:04:51', lv: 'ok', msg: 'plugin <span class="em-ok">reconnected</span> · re-diff vs base' },
      { time: '12:04:51', lv: 'ok', msg: 'lane <span class="lane">FS→Studio</span> <span class="em-ok">resumed</span> · 0 loss' },
    ],
  },

  architecture: {
    num: '05',
    tag: 'Cargo workspace + thin Luau plugin',
    title: 'The brain never touches the network. So it is testable.',
    lead: 'naht-core holds all the reconcile/merge logic with zero I/O — testable in isolation. The daemon owns transport and the disk. The Luau plugin is kept deliberately thin.',
    leadVs: 'Clean boundary: the hard logic lives where it can be tested, not in the plugin.',
    winTitle: 'naht — workspace',
    badgeCore: '0 I/O · testable',
    badgeDaemon: 'daemon · owns I/O',
    badgePlugin: 'Luau · thin',
    tomlTag: 'workspace',
    tomlComment: ['# naht-core depends on', '# no network / fs crate', '# → testable alone'],
    testCmd: 'cargo test -p naht-core',
    testRunning: 'running 38 tests',
    testPass1: 'test reconcile::three_way_clean ... ok',
    testPass2: 'test merge::conflict_freezes_path ... ok',
    testResult: '✓ test result: ok. 38 passed; 0 failed',
    testResultNote: '— without starting a server',
  },

  try: {
    num: '06',
    label: 'Try it',
    title: 'Zero to a confirmed bidirectional sync.',
    lead: 'Convention-first: an optional naht.toml carries only the name, the port, and the place-id guard. The rest follows convention.',
    termTitle: 'zsh — demo',
    term: {
      scaffold: '  scaffolded src/ · naht.toml · .gitignore',
      scaffoldNote: '# --from-rojo to migrate',
      watching: 'watching src/ · localhost:34872',
      connecting: 'Connecting…',
      connected: 'Connected',
      connectedNote: '· source mounted under ServerStorage/Naht',
      editDisk: '# edit src/PlayerData.lua in your editor',
      appliedFs: 'FS→Studio',
      applied: 'applied ✓',
      editStudio: '# edit the ModuleScript in Studio',
      appliedStudioPre: 'Studio→FS',
      mergeLabel: 'merge 3-way',
      mergeResult: '→ base #49 ✓',
      done: 'bidirectional confirmed · 0 conflict · persisted base',
    },
    limitsTitle: 'The API ceiling, surfaced — not hidden',
    limits: [
      { label: 'MeshId / images', status: 'Cloud upload', cls: 'ok' },
      { label: 'Terrain', status: 'voxels', cls: 'ok' },
      { label: 'CSG / Unions', status: 'rbxm round-trip', cls: 'wn' },
      { label: 'HttpEnabled & locked props', status: 'hard block → Game Settings', cls: 'bd' },
    ],
    ctaPrimary: 'Full quickstart',
    ctaSecondary: 'Architecture doc',
  },

  footer: {
    tagline: 'The seam between your filesystem and Roblox Studio.',
    links: { github: 'GitHub', quickstart: 'Quickstart', architecture: 'Architecture', priorArt: 'Prior art' },
    license: 'Dual-licensed MIT or Apache-2.0.',
  },
}
```

- [ ] **Step 4: Create the French dictionary**

Create `site/src/i18n/fr.js` (same key shape; mockup copy):

```js
export default {
  nav: {
    comparison: 'Comparatif',
    merge: 'Merge',
    architecture: 'Architecture',
    quickstart: 'Quickstart',
    github: 'GitHub',
  },
  toggle: { aria: 'Langue' },

  hero: {
    pill: 'Rust · Roblox Studio · bidirectionnel',
    title1: 'La ',
    titleAccent: 'couture',
    title2: ' entre ton disque et Roblox Studio.',
    sub: 'Sync bidirectionnel avec un vrai merge 3-way contre une base persistée. Jamais destructif — un échec d\'écriture met une voie en pause, il ne tue jamais la session.',
    ctaPrimary: 'Quickstart',
    ctaSecondary: 'Voir le sync',
    meta: ['0 unwrap() dans la boucle', '3-way merge texte', 'SQLite base persistée'],
    seam: {
      fsHeader: 'Filesystem',
      studioHeader: 'Studio Explorer',
      status: 'Connected · base ack-gated · re-diff au reconnect',
    },
  },

  matrix: {
    num: '01',
    label: 'Comparatif',
    title: 'Construit autour des modes d\'échec qui rendent les autres pénibles.',
    lead: 'Alternative repensée de zéro à Rojo et Argon. Chaque ligne est ancrée dans un échec concret des outils en place — détaillée juste en dessous.',
    note: 'Source : docs/prior-art.md — teardown sourcé (juin 2026).',
    head: { naht: 'Naht', rojo: 'Rojo', argon: 'Argon' },
    rows: [
      { label: 'Sync bidirectionnel', naht: 'cœur', rojo: 'expérimental', argon: '✓', rojoCls: 'wn', argonCls: 'yes' },
      { label: 'Merge 3-way texte', naht: '✓', rojo: '✗ overwrite', argon: '✗ instance', rojoCls: 'no', argonCls: 'no' },
      { label: 'Base last-sync persistée', naht: 'SQLite', rojo: '✗ mémoire', argon: '✗ mémoire', rojoCls: 'no', argonCls: 'no' },
      { label: 'Reconnect + état visible', naht: '✓', rojo: '✗', argon: 'partiel', rojoCls: 'no', argonCls: 'wn' },
      { label: 'Non-destructif (zéro unwrap)', naht: '✓', rojo: '✗ crash', argon: '~', rojoCls: 'no', argonCls: 'wn' },
      { label: 'Config par convention + couches', naht: '✓', rojo: '✗ verbeux', argon: '~', rojoCls: 'no', argonCls: 'wn' },
      { label: 'Props non-syncables signalées', naht: '✓', rojo: '✗ silencieux', argon: '✗', rojoCls: 'no', argonCls: 'no' },
    ],
  },

  merge: {
    num: '02',
    tag: 'le différenciateur n°1',
    title: 'Un vrai merge 3-way, jamais un gagnant arbitraire.',
    lead: 'Disque et Studio sont mergés contre la base last-sync. Un hunk propre est écrit et la base avance ; un hunk en conflit gèle le path avec des marqueurs git — Naht ne choisit jamais à ta place.',
    leadVs: 'Rojo : overwrite-on-conflict. Argon : résolution au niveau instance, pas du texte.',
    tab: 'PlayerData.lua — Merging (1 conflict)',
    diskHeader: 'DISK',
    diskSub: 'current · à jour',
    studioHeader: 'STUDIO',
    studioSub: 'incoming · édité',
    accept: 'Accept ▸',
    resultHeader: 'RESULT',
    baseTag: 'base last-sync · SQLite',
    conflictBar: 'Conflict 1 — m.speed',
    actionDisk: 'Accept Disk',
    actionStudio: 'Accept Studio',
    actionBoth: 'Both',
    autoMerged: '-- ✓ auto-merged',
    footerClean: '1 hunk propre → base avance',
    footerConflict: '1 conflit → path gelé, jamais auto-résolu',
  },

  base: {
    num: '03',
    tag: 'survit aux restarts',
    title: 'L\'état de réconciliation vit sur le disque, pas en RAM.',
    lead: 'La base last-sync est en SQLite. Un crash, un redémarrage du daemon, une coupure réseau — au retour, Naht re-diffe contre l\'état persisté au lieu de tout re-clobber. Les patchs FS → Studio sont ack-gated : la base n\'avance qu\'une fois le plugin confirmé.',
    leadVs: 'Rojo & Argon : état de sync en mémoire, perdu au restart.',
    inspectorTitle: 'naht — state inspector',
    query: 'SELECT * FROM base ORDER BY synced_at DESC',
    cols: { path: 'path', hash: 'blob_hash', rev: 'rev', ts: 'synced_at' },
    footer: 'daemon restarted · base rechargée depuis le disque · re-diff 142 paths → 0 re-clobber',
  },

  nondestructive: {
    num: '04',
    tag: 'zéro unwrap() dans la boucle',
    title: 'Un échec met une voie en pause. Il ne tue jamais la session.',
    lead: 'Quand une écriture échoue, la voie concernée passe en pause — l\'autre sens continue. Heartbeat, auto-reconnect avec backoff, état toujours visible. La connexion qui meurt n\'est ni silencieuse ni fatale.',
    leadVs: 'Rojo : le two-way peut crasher le serveur (.unwrap()). Argon : sync coupé quand le widget se ferme.',
    termTitle: 'zsh — naht serve -vv',
    logs: [
      { time: '12:04:30', lv: 'info', msg: 'watching <b>src/</b> · plugin <span class="em-ok">connected</span>' },
      { time: '12:04:31', lv: 'err', msg: 'write failed <span class="lane">FS→Studio</span> <b>PlayerData</b> — <span class="em-err">DataModel busy</span>' },
      { time: '12:04:31', lv: 'warn', msg: 'lane <span class="lane">FS→Studio</span> <span class="em-warn">paused</span> · session intacte, pas d\'unwrap' },
      { time: '12:04:31', lv: 'info', msg: 'lane <span class="lane">Studio→FS</span> continue sans interruption' },
      { time: '12:04:46', lv: 'warn', msg: 'connection lost · heartbeat timeout' },
      { time: '12:04:47', lv: 'info', msg: 'reconnect backoff <b>1s → 2s → 4s</b>' },
      { time: '12:04:51', lv: 'ok', msg: 'plugin <span class="em-ok">reconnected</span> · re-diff vs base' },
      { time: '12:04:51', lv: 'ok', msg: 'lane <span class="lane">FS→Studio</span> <span class="em-ok">resumed</span> · 0 perte' },
    ],
  },

  architecture: {
    num: '05',
    tag: 'workspace Cargo + plugin Luau fin',
    title: 'Le cerveau ne touche jamais le réseau. Donc il est testable.',
    lead: 'naht-core contient toute la logique de reconcile/merge avec zéro I/O — testable en isolation. Le daemon possède le transport et le disque. Le plugin Luau est gardé délibérément fin.',
    leadVs: 'Frontière nette : la logique difficile est là où elle se teste, pas dans le plugin.',
    winTitle: 'naht — workspace',
    badgeCore: '0 I/O · testable',
    badgeDaemon: 'daemon · owns I/O',
    badgePlugin: 'Luau · thin',
    tomlTag: 'workspace',
    tomlComment: ['# naht-core ne dépend', '# d\'aucun crate réseau /', '# fs → testable seul'],
    testCmd: 'cargo test -p naht-core',
    testRunning: 'running 38 tests',
    testPass1: 'test reconcile::three_way_clean ... ok',
    testPass2: 'test merge::conflict_freezes_path ... ok',
    testResult: '✓ test result: ok. 38 passed; 0 failed',
    testResultNote: '— sans démarrer de serveur',
  },

  try: {
    num: '06',
    label: 'Essayer',
    title: 'De zéro à une sync bidirectionnelle confirmée.',
    lead: 'Convention-first : un naht.toml optionnel ne porte que le nom, le port et le garde-fou place-id. Le reste suit la convention.',
    termTitle: 'zsh — demo',
    term: {
      scaffold: '  scaffolded src/ · naht.toml · .gitignore',
      scaffoldNote: '# --from-rojo pour migrer',
      watching: 'watching src/ · localhost:34872',
      connecting: 'Connecting…',
      connected: 'Connected',
      connectedNote: '· source montée sous ServerStorage/Naht',
      editDisk: '# edit src/PlayerData.lua dans ton éditeur',
      appliedFs: 'FS→Studio',
      applied: 'applied ✓',
      editStudio: '# edit le ModuleScript dans Studio',
      appliedStudioPre: 'Studio→FS',
      mergeLabel: 'merge 3-way',
      mergeResult: '→ base #49 ✓',
      done: 'bidirectionnel confirmé · 0 conflit · base persistée',
    },
    limitsTitle: 'Le plafond de l\'API, signalé — pas caché',
    limits: [
      { label: 'MeshId / images', status: 'upload Cloud', cls: 'ok' },
      { label: 'Terrain', status: 'voxels', cls: 'ok' },
      { label: 'CSG / Unions', status: 'round-trip rbxm', cls: 'wn' },
      { label: 'HttpEnabled & props verrouillées', status: 'hard block → Game Settings', cls: 'bd' },
    ],
    ctaPrimary: 'Quickstart complet',
    ctaSecondary: 'Doc architecture',
  },

  footer: {
    tagline: 'La couture entre ton filesystem et Roblox Studio.',
    links: { github: 'GitHub', quickstart: 'Quickstart', architecture: 'Architecture', priorArt: 'Prior art' },
    license: 'Dual-licensed MIT ou Apache-2.0.',
  },
}
```

- [ ] **Step 5: Run the full i18n suite — all green**

Run: `cd site && npx vitest run`
Expected: PASS — `index.test.js` and `dictionaries.test.js` all pass.

- [ ] **Step 6: Commit**

```bash
git add site/src/i18n/en.js site/src/i18n/fr.js site/src/i18n/dictionaries.test.js
git commit -m "feat(site): en/fr dictionaries with key-parity test"
```

---

## Task 4: Language toggle

**Files:**
- Create: `site/src/lib/LanguageToggle.svelte`

- [ ] **Step 1: Create the component**

Create `site/src/lib/LanguageToggle.svelte`:

```svelte
<script>
  import { locale, setLocale, LOCALES } from '../i18n/index.js'
</script>

<div class="toggle" role="group" aria-label="Language">
  {#each LOCALES as code (code)}
    <button
      class="opt"
      class:active={$locale === code}
      aria-pressed={$locale === code}
      onclick={() => setLocale(code)}
    >{code.toUpperCase()}</button>
  {/each}
</div>

<style>
  .toggle {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 9px;
    overflow: hidden;
    font-family: var(--mono);
  }
  .opt {
    background: transparent;
    color: var(--text-dim);
    border: none;
    padding: 6px 11px;
    font-size: 0.74rem;
    font-family: inherit;
    cursor: pointer;
    transition: color 0.15s ease, background 0.15s ease;
  }
  .opt:hover { color: var(--text); }
  .opt.active {
    color: #1a1205;
    background: linear-gradient(120deg, var(--fs), #f0a030);
    font-weight: 700;
  }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: build succeeds (component is unused until Task 14 but must compile).

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/LanguageToggle.svelte
git commit -m "feat(site): FR/EN language toggle"
```

---

## Task 5: SeamPanel hero visual (approved floating seam)

**Files:**
- Modify: `site/src/lib/SeamPanel.svelte` (full replacement)

Reproduce the approved hero seam from `fullpage-v8.html` (the `.seamvis` block, lines ~61-97 CSS and the hero markup). Code identifiers stay literal; the two prose labels and the status come from `$t.hero.seam`.

- [ ] **Step 1: Replace the component**

Overwrite `site/src/lib/SeamPanel.svelte`:

```svelte
<script>
  import { t } from '../i18n/index.js'
</script>

<div class="seamvis">
  <div class="lace">
    <div class="pane d">
      <div class="phd"><span class="dotwin"><i></i><i></i><i></i></span> {$t.hero.seam.fsHeader} <span class="tag">PlayerData.lua</span></div>
      <div class="code">
        <div class="cl"><span class="gut">1</span><span><span class="kw">local</span> <span class="id">m</span> <span class="pn">=</span> <span class="pn">{'{}'}</span></span></div>
        <div class="cl hot"><span class="gut">2</span><span><span class="id">m</span><span class="pn">.</span><span class="fn2">speed</span> <span class="pn">=</span> <span class="num">16</span></span></div>
        <div class="cl"><span class="gut">3</span><span><span class="id">m</span><span class="pn">.</span><span class="fn2">jump</span> <span class="pn">=</span> <span class="num">50</span></span></div>
        <div class="cl"><span class="gut">4</span><span><span class="id">m</span><span class="pn">.</span><span class="fn2">dash</span> <span class="pn">=</span> <span class="bool">true</span></span></div>
        <div class="cl"><span class="gut">5</span><span></span></div>
        <div class="cl"><span class="gut">6</span><span><span class="kw">return</span> <span class="id">m</span></span></div>
      </div>
    </div>

    <div class="gap">
      <svg viewBox="0 0 168 272">
        <defs><linearGradient id="lr" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#f5b54a" /><stop offset=".5" stop-color="#9aa0ff" /><stop offset="1" stop-color="#5cc8ff" />
        </linearGradient></defs>
        <path class="t2" stroke="url(#lr)" d="M10,56 C84,56 84,136 158,136 M10,136 C84,136 84,216 158,216 M10,216 C84,216 84,56 158,56" />
        <path class="t2dash" stroke="url(#lr)" d="M10,56 C84,56 84,136 158,136 M10,136 C84,136 84,216 158,216 M10,216 C84,216 84,56 158,56" />
        <circle class="hole d" cx="10" cy="56" r="3.6" /><circle class="hole d" cx="10" cy="136" r="3.6" /><circle class="hole d" cx="10" cy="216" r="3.6" />
        <circle class="hole s" cx="158" cy="56" r="3.6" /><circle class="hole s" cx="158" cy="136" r="3.6" /><circle class="hole s" cx="158" cy="216" r="3.6" />
      </svg>
      <div class="chip toS">.lua</div>
      <div class="chip toD">.lua</div>
    </div>

    <div class="pane s">
      <div class="phd">{$t.hero.seam.studioHeader} <span class="tag">ServerStorage</span></div>
      <div class="tree">
        <div class="tr"><span class="tw">▾</span><span class="ico svc"></span> ServerStorage</div>
        <div class="tr l2"><span class="tw">▾</span><span class="ico fold"></span> Naht</div>
        <div class="tr l3 hot"><span class="tw"></span><span class="ico mod"></span> PlayerData</div>
        <div class="tr l3"><span class="tw"></span><span class="ico mod"></span> Shop</div>
        <div class="tr l3"><span class="tw"></span><span class="ico mod"></span> Combat</div>
        <div class="tr l3"><span class="tw"></span><span class="ico mod"></span> Util</div>
      </div>
    </div>
  </div>
  <div class="status"><span class="g"></span> {$t.hero.seam.status}</div>
</div>

<style>
  .seamvis { position: relative; }
  .seamvis::before { content: ''; position: absolute; inset: -8% -4%; z-index: -1; background: radial-gradient(60% 60% at 50% 50%, rgba(154,160,255,.10), transparent 70%); filter: blur(8px); }
  .lace { display: grid; grid-template-columns: 1fr 168px 1fr; align-items: center; }
  .pane { border: 1px solid var(--border); border-radius: 16px; background: linear-gradient(180deg,#0d0f15,#0a0b0e); overflow: hidden; box-shadow: 0 40px 80px -50px rgba(0,0,0,.9); }
  .pane.d { box-shadow: 0 40px 80px -48px rgba(245,181,74,.25), 0 30px 60px -50px #000; }
  .pane.s { box-shadow: 0 40px 80px -48px rgba(92,200,255,.25), 0 30px 60px -50px #000; }
  .phd { display: flex; align-items: center; gap: 8px; padding: 13px 15px; border-bottom: 1px solid var(--border-soft); font-family: var(--mono); font-size: .64rem; letter-spacing: .05em; text-transform: uppercase; }
  .pane.d .phd { color: var(--fs); } .pane.s .phd { color: var(--studio); }
  .phd .tag { margin-left: auto; color: var(--text-faint); text-transform: none; }
  .dotwin { display: flex; gap: 5px; } .dotwin i { width: 9px; height: 9px; border-radius: 50%; background: var(--border-strong); }
  .code { padding: 14px 0; font-family: var(--mono); font-size: .76rem; line-height: 1; }
  .cl { display: grid; grid-template-columns: 30px 1fr; align-items: center; height: 34px; padding-right: 16px; }
  .cl .gut { color: #3a3f4a; text-align: right; padding-right: 12px; font-size: .66rem; }
  .cl.hot { background: linear-gradient(90deg, rgba(245,181,74,.16), transparent); }
  .kw { color: #d98ad9; } .id { color: var(--text); } .num { color: #7fd6a6; } .fn2 { color: var(--fs); } .pn { color: var(--text-dim); } .bool { color: #7fd6a6; }
  .tree { padding: 14px 0; font-family: var(--mono); font-size: .76rem; }
  .tr { display: flex; align-items: center; gap: 8px; height: 34px; padding: 0 16px; color: var(--text-dim); }
  .tr .tw { color: var(--text-faint); width: 10px; } .tr .ico { width: 13px; height: 13px; border-radius: 3px; }
  .ico.svc { background: rgba(92,200,255,.25); border: 1px solid rgba(92,200,255,.5); }
  .ico.fold { background: rgba(154,160,255,.22); border: 1px solid rgba(154,160,255,.5); }
  .ico.mod { background: rgba(92,200,255,.35); border: 1px solid var(--studio); }
  .tr.l2 { padding-left: 32px; } .tr.l3 { padding-left: 50px; }
  .tr.hot { background: linear-gradient(90deg, transparent, rgba(92,200,255,.18)); color: var(--text); }
  .gap { position: relative; width: 168px; height: 272px; justify-self: center; }
  .gap svg { position: absolute; inset: 0; width: 168px; height: 272px; overflow: visible; }
  .hole { fill: #070809; stroke-width: 1.7; } .hole.d { stroke: var(--fs); } .hole.s { stroke: var(--studio); }
  .t2 { stroke-width: 2; fill: none; stroke-linecap: round; opacity: .4; filter: drop-shadow(0 0 4px rgba(154,160,255,.5)); }
  .t2dash { stroke-width: 2.2; fill: none; stroke-dasharray: 5 300; stroke-linecap: round; animation: bead 3s linear infinite; }
  @keyframes bead { to { stroke-dashoffset: -305; } }
  .chip { position: absolute; top: 0; left: 0; width: 42px; height: 25px; border-radius: 8px; display: grid; place-items: center; font-family: var(--mono); font-size: .56rem; font-weight: 700; color: #0c0d10; offset-rotate: 0deg; offset-anchor: 50% 50%; }
  .chip.toS { background: linear-gradient(120deg, var(--fs), var(--merge)); box-shadow: 0 0 22px rgba(245,181,74,.8); offset-path: path('M10 56 C84 56 84 136 158 136'); animation: ride 3.6s ease-in-out infinite; }
  .chip.toD { background: linear-gradient(120deg, var(--studio), var(--merge)); box-shadow: 0 0 22px rgba(92,200,255,.8); offset-path: path('M158 216 C84 216 84 136 10 136'); animation: ride 3.6s ease-in-out infinite; animation-delay: 1.8s; }
  @keyframes ride { 0% { offset-distance: 0%; opacity: 0; } 10% { opacity: 1; } 90% { opacity: 1; } 100% { offset-distance: 100%; opacity: 0; } }
  .status { display: flex; align-items: center; gap: 10px; justify-content: center; margin-top: 22px; font-family: var(--mono); font-size: .74rem; color: var(--ok); }
  .status .g { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 10px var(--ok); }
  @media (prefers-reduced-motion: reduce) {
    .t2dash, .chip.toS, .chip.toD { animation: none; }
    .chip.toS { offset-distance: 50%; } .chip.toD { offset-distance: 50%; }
  }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/SeamPanel.svelte
git commit -m "feat(site): approved floating-seam hero visual, i18n-wired"
```

---

## Task 6: Hero copy via i18n

**Files:**
- Modify: `site/src/lib/Hero.svelte` (full replacement)

- [ ] **Step 1: Replace the component**

Overwrite `site/src/lib/Hero.svelte`:

```svelte
<script>
  import Icon from './Icon.svelte'
  import SeamPanel from './SeamPanel.svelte'
  import { reveal } from './reveal.js'
  import { t } from '../i18n/index.js'

  const REPO = 'https://github.com/vskstudio/naht'
</script>

<header id="top" class="hero">
  <div class="wrap hero-grid">
    <div class="copy">
      <span class="pill reveal" use:reveal><span class="g"></span> {$t.hero.pill}</span>
      <h1 class="reveal" use:reveal={{ delay: 60 }}>{$t.hero.title1}<span class="grad">{$t.hero.titleAccent}</span>{$t.hero.title2}</h1>
      <p class="hero-sub reveal" use:reveal={{ delay: 120 }}>{$t.hero.sub}</p>
      <div class="cta-row reveal" use:reveal={{ delay: 180 }}>
        <a class="btn primary" href="#start"><Icon name="arrow" size={17} /> {$t.hero.ctaPrimary}</a>
        <a class="btn ghost" href="#sync"><Icon name="sync" size={17} /> {$t.hero.ctaSecondary}</a>
      </div>
      <div class="hero-meta reveal" use:reveal={{ delay: 240 }}>
        {#each $t.hero.meta as m (m)}<span>{m}</span>{/each}
      </div>
    </div>
    <div class="hero-visual reveal" use:reveal={{ delay: 160 }}>
      <SeamPanel />
    </div>
  </div>
</header>

<style>
  .hero { padding: 84px 0 104px; }
  .hero-grid { display: grid; grid-template-columns: 1fr 1.15fr; gap: 80px; align-items: center; }
  .pill { display: inline-flex; align-items: center; gap: 10px; font-family: var(--mono); font-size: .74rem; letter-spacing: .05em; color: var(--text-dim); border: 1px solid var(--border); background: #0a0b0e; border-radius: 999px; padding: 6px 13px; margin-bottom: 26px; }
  .pill .g { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 9px var(--ok); }
  h1 { font-size: clamp(2.6rem, 4.4vw, 4.5rem); letter-spacing: -.04em; line-height: 1.02; }
  .hero-sub { color: var(--text-dim); font-size: 1.2rem; max-width: 48ch; margin: 24px 0 34px; }
  .cta-row { display: flex; gap: 14px; flex-wrap: wrap; }
  .hero-meta { display: flex; gap: 22px; margin-top: 30px; font-family: var(--mono); font-size: .74rem; color: var(--text-faint); flex-wrap: wrap; }
  .hero-visual { min-width: 0; }
  @media (max-width: 900px) { .hero-grid { grid-template-columns: 1fr; gap: 48px; } }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/Hero.svelte
git commit -m "feat(site): hero copy via i18n, embeds new SeamPanel"
```

---

## Task 7: FeatureRow — numbered eyebrow

**Files:**
- Modify: `site/src/lib/FeatureRow.svelte`

The new blocks use an index eyebrow (`01 Comparatif ——`) and a pill tag, not the old icon eyebrow. Extend FeatureRow to support a `num` + `label` index header and an optional `tag`/`tagCls`, while keeping the slot API.

- [ ] **Step 1: Replace the component**

Overwrite `site/src/lib/FeatureRow.svelte`:

```svelte
<script>
  import { reveal } from './reveal.js'
  export let num = ''
  export let label = ''
  export let title = ''
  export let tag = ''
  export let tagCls = ''
  export let flip = false
  export let id = undefined
</script>

<section class="row" class:flip {id}>
  <div class="copy">
    {#if num}
      <div class="idx reveal" use:reveal><span class="n">{num}</span> {label} <span class="bar"></span></div>
    {/if}
    {#if tag}<span class="ftag {tagCls} reveal" use:reveal>{tag}</span>{/if}
    <h2 class="section-title reveal" use:reveal>{title}</h2>
    <div class="section-lead reveal" use:reveal><slot name="lead" /></div>
  </div>
  <div class="visual reveal" use:reveal={{ delay: 120 }}>
    <slot />
  </div>
</section>

<style>
  .row { display: grid; grid-template-columns: 1fr 1.08fr; gap: 72px; align-items: center; max-width: var(--maxw); margin: 0 auto; padding: 92px 24px; }
  .row.flip .copy { order: 2; } .row.flip .visual { order: 1; }
  .visual { min-width: 0; }
  .idx { display: flex; align-items: center; gap: 12px; font-family: var(--mono); font-size: .72rem; letter-spacing: .16em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 18px; }
  .idx .n { color: var(--fs); } .idx .bar { flex: 1; height: 1px; max-width: 64px; background: var(--border-strong); }
  .ftag { display: inline-flex; align-items: center; gap: 8px; font-family: var(--mono); font-size: .72rem; letter-spacing: .04em; color: var(--fs); background: rgba(245,181,74,.1); border: 1px solid rgba(245,181,74,.22); padding: 4px 11px; border-radius: 999px; margin-bottom: 18px; }
  .ftag.m { color: var(--merge); background: rgba(154,160,255,.1); border-color: rgba(154,160,255,.22); }
  .ftag.s { color: var(--studio); background: rgba(92,200,255,.1); border-color: rgba(92,200,255,.22); }
  .ftag.o { color: var(--ok); background: rgba(74,210,149,.1); border-color: rgba(74,210,149,.22); }
  @media (max-width: 900px) { .row, .row.flip { grid-template-columns: 1fr; gap: 40px; padding: 64px 24px; } .row.flip .copy, .row.flip .visual { order: 0; } }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/FeatureRow.svelte
git commit -m "feat(site): FeatureRow numbered eyebrow + tag"
```

---

## Task 8: ComparisonMatrix (block 01)

**Files:**
- Create: `site/src/lib/ComparisonMatrix.svelte`

`naht` column cell uses the `yes` style; rojo/argon cells use the `*Cls` from the row data (`yes`/`wn`/`no`).

- [ ] **Step 1: Create the component**

```svelte
<script>
  import { t } from '../i18n/index.js'
</script>

<div class="mx">
  <table>
    <colgroup><col class="cn" /><col /><col /><col /></colgroup>
    <thead>
      <tr><th></th><th class="naht">{$t.matrix.head.naht}</th><th>{$t.matrix.head.rojo}</th><th>{$t.matrix.head.argon}</th></tr>
    </thead>
    <tbody>
      {#each $t.matrix.rows as r (r.label)}
        <tr>
          <th>{r.label}</th>
          <td class="naht"><span class="yes">{r.naht}</span></td>
          <td class={r.rojoCls}>{r.rojo}</td>
          <td class={r.argonCls}>{r.argon}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
<p class="mx-note">{$t.matrix.note}</p>

<style>
  .mx { border: 1px solid var(--border); border-radius: 16px; overflow: hidden; background: var(--bg-card); }
  table { border-collapse: collapse; width: 100%; font-size: .94rem; }
  th, td { padding: 15px 18px; text-align: center; border-top: 1px solid var(--border-soft); }
  thead th { background: #0a0b0e; font-weight: 700; border-top: none; position: relative; }
  tbody th { text-align: left; color: #cdd2db; font-weight: 500; }
  col.cn { width: 34%; }
  .naht { background: rgba(245,181,74,.055); }
  thead .naht { color: var(--fs); }
  thead .naht::after { content: ''; position: absolute; left: 0; right: 0; top: 0; height: 2px; background: var(--grad); }
  .yes { color: var(--ok); font-weight: 700; } .wn { color: var(--fs); font-weight: 700; } .no { color: var(--danger); font-weight: 700; opacity: .85; }
  .naht .yes { font-family: var(--mono); font-size: .82rem; }
  .mx-note { margin-top: 13px; font-family: var(--mono); font-size: .72rem; color: var(--text-faint); }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/ComparisonMatrix.svelte
git commit -m "feat(site): comparison matrix (block 01)"
```

---

## Task 9: MergeEditor (block 02)

**Files:**
- Create: `site/src/lib/MergeEditor.svelte`

Reproduce the VS Code merge editor from `fullpage-v8.html` / `merge-editor.html`. Code tokens are literal; the prose (headers, tab, conflict bar, actions, footer) comes from `$t.merge`.

- [ ] **Step 1: Create the component**

```svelte
<script>
  import { t } from '../i18n/index.js'
</script>

<div class="vsc">
  <div class="titlebar"><span class="dots"><i></i><i></i><i></i></span><span class="title">naht — merge editor</span></div>
  <div class="tabrow"><span class="tab"><span class="fi"></span> {$t.merge.tab}</span></div>

  <div class="inputs">
    <div class="vpane disk">
      <div class="vphd"><span class="d"></span> {$t.merge.diskHeader} <span class="sub">{$t.merge.diskSub}</span> <span class="accept">{$t.merge.accept}</span></div>
      <div class="ved">
        <div class="vln"><span class="g">1</span><span><span class="vkw">local</span> <span class="vid">m</span> <span class="vpn">=</span> <span class="vpn">{'{}'}</span></span></div>
        <div class="vln cur"><span class="g">2</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">speed</span> <span class="vpn">=</span> <span class="vnum">24</span></span></div>
        <div class="vln"><span class="g">3</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">jump</span> <span class="vpn">=</span> <span class="vnum">50</span></span></div>
      </div>
    </div>
    <div class="vpane studio">
      <div class="vphd"><span class="d"></span> {$t.merge.studioHeader} <span class="sub">{$t.merge.studioSub}</span> <span class="accept">{$t.merge.accept}</span></div>
      <div class="ved">
        <div class="vln"><span class="g">1</span><span><span class="vkw">local</span> <span class="vid">m</span> <span class="vpn">=</span> <span class="vpn">{'{}'}</span></span></div>
        <div class="vln inc"><span class="g">2</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">speed</span> <span class="vpn">=</span> <span class="vnum">32</span></span></div>
        <div class="vln inc"><span class="g">3</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">jump</span> <span class="vpn">=</span> <span class="vnum">65</span></span></div>
      </div>
    </div>
  </div>

  <div class="result">
    <div class="rhd"><span class="d"></span> {$t.merge.resultHeader} <span class="base"><span class="cyl"></span> {$t.merge.baseTag}</span></div>
    <div class="ved">
      <div class="vln"><span class="g">1</span><span><span class="vkw">local</span> <span class="vid">m</span> <span class="vpn">=</span> <span class="vpn">{'{}'}</span></span></div>
      <div class="cf">
        <div class="cfbar">⟂ {$t.merge.conflictBar}
          <span class="actions"><span class="a cur">{$t.merge.actionDisk}</span><span class="a inc">{$t.merge.actionStudio}</span><span class="a">{$t.merge.actionBoth}</span></span>
        </div>
        <div class="cfmark h-cur"><span class="g">◂</span><span>&lt;&lt;&lt;&lt;&lt;&lt;&lt; disk (current)</span></div>
        <div class="cfmark h-cur"><span class="g">2</span><span>m.speed = 24</span></div>
        <div class="cfmark sep"><span class="g"></span><span>=======</span></div>
        <div class="cfmark h-inc"><span class="g">2</span><span>m.speed = 32</span></div>
        <div class="cfmark h-inc"><span class="g">▸</span><span>&gt;&gt;&gt;&gt;&gt;&gt;&gt; studio (incoming)</span></div>
      </div>
      <div class="vln ok"><span class="g gut-ok">3</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">jump</span> <span class="vpn">=</span> <span class="vnum">65</span>  <span class="vcm">{$t.merge.autoMerged}</span></span></div>
      <div class="vln"><span class="g">4</span><span><span class="vkw">return</span> <span class="vid">m</span></span></div>
    </div>
    <div class="vfoot"><span class="gd"></span> {$t.merge.footerClean} <span class="sep">·</span> <span class="frozen">{$t.merge.footerConflict}</span></div>
  </div>
</div>

<style>
  .vsc { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; box-shadow: 0 40px 90px -55px rgba(124,131,255,.45), 0 30px 60px -50px #000; font-family: var(--mono); }
  .titlebar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .title { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .tabrow { display: flex; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .tab { display: inline-flex; align-items: center; gap: 7px; padding: 6px 13px; background: #0b0d12; border: 1px solid var(--border-soft); border-bottom: none; border-radius: 7px 7px 0 0; font-size: .72rem; color: var(--text); margin: 6px 0 -1px 14px; }
  .tab .fi { width: 11px; height: 11px; border-radius: 3px; background: var(--grad); }
  .inputs { display: grid; grid-template-columns: 1fr 1fr; border-bottom: 1px solid var(--border-soft); }
  .vpane { border-right: 1px solid var(--border-soft); } .vpane:last-child { border-right: none; }
  .vphd { display: flex; align-items: center; gap: 8px; padding: 9px 14px; font-size: .66rem; letter-spacing: .04em; background: #0a0c10; border-bottom: 1px solid var(--border-soft); }
  .vphd .d { width: 8px; height: 8px; border-radius: 50%; }
  .vpane.disk .vphd .d { background: var(--fs); } .vpane.disk .vphd { color: var(--fs); }
  .vpane.studio .vphd .d { background: var(--studio); } .vpane.studio .vphd { color: var(--studio); }
  .vphd .sub { margin-left: auto; color: var(--text-faint); text-transform: none; letter-spacing: 0; }
  .accept { font-size: .6rem; color: var(--text-faint); border: 1px solid var(--border-soft); border-radius: 5px; padding: 2px 7px; }
  .ved { font-size: .74rem; line-height: 1; padding: 6px 0; }
  .vln { display: grid; grid-template-columns: 34px 1fr; align-items: center; height: 26px; }
  .vln .g { color: #39414e; text-align: right; padding-right: 11px; font-size: .64rem; }
  .vln.cur { background: linear-gradient(90deg, rgba(245,181,74,.14), transparent); box-shadow: inset 2px 0 0 var(--fs); }
  .vln.inc { background: linear-gradient(90deg, rgba(92,200,255,.14), transparent); box-shadow: inset 2px 0 0 var(--studio); }
  .vln.ok { background: linear-gradient(90deg, rgba(74,210,149,.10), transparent); box-shadow: inset 2px 0 0 var(--ok); }
  .vkw { color: #d98ad9; } .vid { color: var(--text); } .vnum { color: #7fd6a6; } .vfn { color: var(--fs); } .vpn { color: var(--text-dim); } .vcm { color: #5b6472; }
  .gut-ok { color: var(--ok) !important; }
  .rhd { display: flex; align-items: center; gap: 9px; padding: 9px 14px; font-size: .66rem; color: var(--merge); background: rgba(154,160,255,.06); border-bottom: 1px solid var(--border-soft); }
  .rhd .d { width: 8px; height: 8px; border-radius: 50%; background: var(--merge); }
  .rhd .base { margin-left: auto; color: var(--text-faint); font-size: .62rem; display: flex; align-items: center; gap: 6px; }
  .rhd .cyl { width: 13px; height: 16px; border-radius: 50%/22%; border: 1px solid rgba(154,160,255,.5); background: linear-gradient(180deg, rgba(154,160,255,.2), transparent); }
  .cf { border-top: 1px solid var(--border-soft); border-bottom: 1px solid var(--border-soft); }
  .cfbar { display: flex; align-items: center; gap: 10px; padding: 7px 14px; font-size: .62rem; background: rgba(242,97,106,.07); color: var(--danger); }
  .cfbar .actions { margin-left: auto; display: flex; gap: 8px; }
  .cfbar .a { padding: 2px 8px; border-radius: 5px; border: 1px solid var(--border-soft); color: var(--text-dim); }
  .cfbar .a.cur { color: var(--fs); border-color: rgba(245,181,74,.4); }
  .cfbar .a.inc { color: var(--studio); border-color: rgba(92,200,255,.4); }
  .cfmark { display: grid; grid-template-columns: 34px 1fr; align-items: center; height: 24px; font-size: .7rem; }
  .cfmark .g { color: #39414e; text-align: right; padding-right: 11px; font-size: .62rem; }
  .cfmark.h-cur { background: rgba(245,181,74,.10); color: var(--fs); }
  .cfmark.h-inc { background: rgba(92,200,255,.10); color: var(--studio); }
  .cfmark.sep { color: var(--text-faint); }
  .vfoot { display: flex; align-items: center; gap: 9px; padding: 9px 14px; font-size: .64rem; color: var(--ok); background: #0a0c10; border-top: 1px solid var(--border-soft); }
  .vfoot .gd { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 8px var(--ok); }
  .vfoot .sep { color: var(--text-faint); } .vfoot .frozen { color: var(--danger); }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/MergeEditor.svelte
git commit -m "feat(site): VS Code merge editor (block 02)"
```

---

## Task 10: StateInspector (block 03)

**Files:**
- Create: `site/src/lib/StateInspector.svelte`

The 4 table rows are literal technical data (paths/hashes/revs/timestamps) held in the component; column headers, title, query, and footer come from `$t.base`.

- [ ] **Step 1: Create the component**

```svelte
<script>
  import { t } from '../i18n/index.js'

  const rows = [
    { path: 'src/PlayerData.lua', hash: 'a3f1c0…', rev: '#48', ts: '12:04:31', changed: true },
    { path: 'src/Shop.lua', hash: '7be902…', rev: '#46', ts: '12:01:08', changed: false },
    { path: 'src/Combat/init.lua', hash: 'd51aa7…', rev: '#46', ts: '12:01:08', changed: false },
    { path: 'src/Util/math.lua', hash: '0c8e34…', rev: '#39', ts: '11:52:55', changed: false },
  ]
</script>

<div class="win">
  <div class="tbar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">{$t.base.inspectorTitle}</span></div>
  <div class="dbtb"><span class="cyl"></span> base <span class="path">.naht/state.db</span> <span class="q">{$t.base.query}</span></div>
  <table class="tbl">
    <thead><tr><th>{$t.base.cols.path}</th><th>{$t.base.cols.hash}</th><th>{$t.base.cols.rev}</th><th>{$t.base.cols.ts}</th></tr></thead>
    <tbody>
      {#each rows as r (r.path)}
        <tr class:changed={r.changed}>
          <td class="pk">{r.path}</td><td class="hash">{r.hash}</td><td class="rev">{r.rev}</td><td class="ts">{r.ts}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  <div class="dbfoot"><span class="gd"></span> {$t.base.footer}</div>
</div>

<style>
  .win { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; font-family: var(--mono); box-shadow: 0 40px 90px -55px rgba(124,131,255,.4), 0 30px 60px -50px #000; }
  .tbar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .ttl { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .dbtb { display: flex; align-items: center; gap: 10px; padding: 9px 14px; font-size: .66rem; color: var(--merge); background: rgba(154,160,255,.06); border-bottom: 1px solid var(--border-soft); }
  .dbtb .cyl { width: 13px; height: 16px; border-radius: 50%/22%; border: 1px solid rgba(154,160,255,.5); background: linear-gradient(180deg, rgba(154,160,255,.2), transparent); }
  .dbtb .path { color: var(--text-faint); } .dbtb .q { margin-left: auto; color: var(--text-faint); font-size: .62rem; }
  .tbl { width: 100%; border-collapse: collapse; font-size: .74rem; }
  .tbl th { text-align: left; padding: 9px 14px; color: var(--text-faint); font-weight: 500; font-size: .62rem; letter-spacing: .04em; background: #0a0c10; border-bottom: 1px solid var(--border-soft); }
  .tbl td { padding: 9px 14px; border-bottom: 1px solid var(--border-soft); color: var(--text-dim); }
  .tbl tr:last-child td { border-bottom: none; }
  .tbl .pk { color: var(--text); } .tbl .hash { color: var(--studio); } .tbl .rev { color: var(--fs); } .tbl .ts { color: var(--text-faint); }
  .tbl tr.changed td { background: rgba(245,181,74,.05); }
  .dbfoot { display: flex; align-items: center; gap: 9px; padding: 10px 14px; font-size: .64rem; color: var(--ok); background: #0a0c10; border-top: 1px solid var(--border-soft); }
  .dbfoot .gd { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 8px var(--ok); }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/StateInspector.svelte
git commit -m "feat(site): SQLite state inspector (block 03)"
```

---

## Task 11: LogStream (block 04)

**Files:**
- Create: `site/src/lib/LogStream.svelte`

Each log line's `msg` is dictionary HTML (inline `<b>`/`<span class>` for emphasis) rendered with `{@html}`. The content is author-controlled (from our own dictionaries), so `{@html}` is safe here.

- [ ] **Step 1: Create the component**

```svelte
<script>
  import { t } from '../i18n/index.js'
</script>

<div class="win">
  <div class="tbar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">{$t.nondestructive.termTitle}</span></div>
  <div class="log">
    {#each $t.nondestructive.logs as line, i (i)}
      <div class="lr">
        <span class="time">{line.time}</span>
        <span class="lv {line.lv}">{line.lv.toUpperCase()}</span>
        <span class="msg">{@html line.msg}{#if i === $t.nondestructive.logs.length - 1}<span class="logcur"></span>{/if}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .win { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; font-family: var(--mono); box-shadow: 0 40px 90px -55px rgba(124,131,255,.4), 0 30px 60px -50px #000; }
  .tbar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .ttl { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .log { padding: 16px 18px; font-size: .76rem; line-height: 1.9; background: #08090d; }
  .lr { display: grid; grid-template-columns: 70px 58px 1fr; gap: 12px; align-items: baseline; }
  .time { color: #39414e; font-size: .66rem; }
  .lv { font-size: .6rem; padding: 1px 7px; border-radius: 5px; justify-self: start; align-self: center; }
  .lv.info { background: rgba(92,200,255,.12); color: var(--studio); }
  .lv.warn { background: rgba(245,181,74,.14); color: var(--fs); }
  .lv.err { background: rgba(242,97,106,.14); color: var(--danger); }
  .lv.ok { background: rgba(74,210,149,.14); color: var(--ok); }
  .msg { color: var(--text-dim); }
  .msg :global(b) { color: var(--text); font-weight: 500; }
  .msg :global(.lane) { color: var(--merge); }
  .msg :global(.em-ok) { color: var(--ok); }
  .msg :global(.em-warn) { color: var(--fs); }
  .msg :global(.em-err) { color: var(--danger); }
  .logcur { display: inline-block; width: 7px; height: 13px; background: var(--ok); vertical-align: -2px; margin-left: 5px; animation: blink 1s steps(2) infinite; }
  @keyframes blink { 50% { opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .logcur { animation: none; } }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/LogStream.svelte
git commit -m "feat(site): naht serve -vv log stream (block 04)"
```

---

## Task 12: WorkspaceTree (block 05)

**Files:**
- Create: `site/src/lib/WorkspaceTree.svelte`

Tree node names / filenames are literal; badges, toml comment, and test ribbon strings come from `$t.architecture`.

- [ ] **Step 1: Create the component**

```svelte
<script>
  import { t } from '../i18n/index.js'
</script>

<div class="win">
  <div class="tbar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">{$t.architecture.winTitle}</span></div>
  <div class="ws">
    <div class="wtree">
      <div class="row"><span class="ico dir"></span><span class="nm">naht/</span></div>
      <div class="row l2"><span class="ico file"></span> Cargo.toml <span class="tag d">{$t.architecture.tomlTag}</span></div>
      <div class="row l2"><span class="ico crate-core"></span><span class="nm">naht-core/</span> <span class="tag core">{$t.architecture.badgeCore}</span></div>
      <div class="row l3"><span class="ico file"></span> reconcile.rs</div>
      <div class="row l3"><span class="ico file"></span> merge.rs</div>
      <div class="row l3"><span class="ico file"></span> diff.rs</div>
      <div class="row l2"><span class="ico crate-d"></span><span class="nm">naht/</span> <span class="tag d">{$t.architecture.badgeDaemon}</span></div>
      <div class="row l3"><span class="ico file"></span> serve.rs</div>
      <div class="row l3"><span class="ico file"></span> watch.rs</div>
      <div class="row l3"><span class="ico file"></span> transport.rs</div>
      <div class="row l2"><span class="ico crate-p"></span><span class="nm">plugin/</span> <span class="tag p">{$t.architecture.badgePlugin}</span></div>
      <div class="row l3"><span class="ico file"></span> init.luau</div>
    </div>
    <div>
      <div class="wcode">
        <div class="cl"><span class="g">1</span><span><span class="s-sec">[workspace]</span></span></div>
        <div class="cl"><span class="g">2</span><span><span class="s-key">members</span> = [<span class="s-str">"naht-core"</span>, <span class="s-str">"naht"</span>]</span></div>
        <div class="cl"><span class="g">3</span><span></span></div>
        {#each $t.architecture.tomlComment as c, i (i)}
          <div class="cl"><span class="g">{i + 4}</span><span><span class="s-cm">{c}</span></span></div>
        {/each}
      </div>
      <div class="wfoot">
        <span class="p">$</span><span class="cmd">{$t.architecture.testCmd}</span><br />
        <span class="dim">{$t.architecture.testRunning}</span><br />
        <span class="pass">{$t.architecture.testPass1}</span><br />
        <span class="pass">{$t.architecture.testPass2}</span><br />
        <span class="res">{$t.architecture.testResult}</span> <span class="dim">{$t.architecture.testResultNote}</span>
      </div>
    </div>
  </div>
</div>

<style>
  .win { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; font-family: var(--mono); box-shadow: 0 40px 90px -55px rgba(124,131,255,.4), 0 30px 60px -50px #000; }
  .tbar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .ttl { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .ws { display: grid; grid-template-columns: 1fr 1fr; }
  .wtree { border-right: 1px solid var(--border-soft); padding: 14px 0; font-size: .78rem; }
  .row { display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 16px; color: var(--text-dim); }
  .ico { width: 13px; height: 13px; border-radius: 3px; flex: none; }
  .ico.dir { background: rgba(154,160,255,.2); border: 1px solid rgba(154,160,255,.45); }
  .ico.crate-core { background: rgba(74,210,149,.22); border: 1px solid rgba(74,210,149,.5); }
  .ico.crate-d { background: rgba(154,160,255,.22); border: 1px solid rgba(154,160,255,.5); }
  .ico.crate-p { background: rgba(92,200,255,.22); border: 1px solid rgba(92,200,255,.5); }
  .ico.file { background: #1b1f27; border: 1px solid var(--border-strong); }
  .row.l2 { padding-left: 32px; } .row.l3 { padding-left: 50px; }
  .nm { color: var(--text); }
  .tag { margin-left: auto; font-size: .58rem; padding: 2px 8px; border-radius: 999px; }
  .tag.core { background: rgba(74,210,149,.12); color: var(--ok); }
  .tag.d { background: rgba(154,160,255,.12); color: var(--merge); }
  .tag.p { background: rgba(92,200,255,.12); color: var(--studio); }
  .wcode { padding: 14px 0; font-size: .74rem; line-height: 1.05; background: #0a0c10; }
  .wcode .cl { display: grid; grid-template-columns: 26px 1fr; align-items: center; height: 24px; padding-right: 14px; }
  .wcode .cl .g { color: #39414e; text-align: right; padding-right: 10px; font-size: .6rem; }
  .s-sec { color: var(--studio); } .s-key { color: var(--fs); } .s-str { color: #7fd6a6; } .s-cm { color: #5b6472; }
  .wfoot { border-top: 1px solid var(--border-soft); background: #08090d; padding: 12px 16px; font-size: .72rem; line-height: 1.75; }
  .wfoot .p { color: var(--fs); margin-right: 7px; } .wfoot .cmd { color: var(--text); }
  .wfoot .pass { color: var(--ok); } .wfoot .dim { color: var(--text-faint); } .wfoot .res { color: var(--ok); }
  @media (max-width: 900px) { .ws { grid-template-columns: 1fr; } .wtree { border-right: none; border-bottom: 1px solid var(--border-soft); } }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/WorkspaceTree.svelte
git commit -m "feat(site): Cargo workspace tree + cargo test (block 05)"
```

---

## Task 13: GuidedTerminal (block 06)

**Files:**
- Create: `site/src/lib/GuidedTerminal.svelte`

The command names / file paths are literal; prose (scaffold note, watching, connecting, etc.) comes from `$t.try.term`.

- [ ] **Step 1: Create the component**

```svelte
<script>
  import { t } from '../i18n/index.js'
</script>

<div class="win">
  <div class="tbar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">{$t.try.termTitle}</span></div>
  <div class="gterm">
    <div class="l"><span class="gp">$</span> <span class="gcmd">naht init demo</span></div>
    <div class="l"><span class="gout">{$t.try.term.scaffold}</span> <span class="gcm">{$t.try.term.scaffoldNote}</span></div>
    <div class="blank"></div>
    <div class="l"><span class="gp">$</span> <span class="gcmd">naht serve</span></div>
    <div class="l"><span class="gtag daemon">daemon</span><span class="gout">{$t.try.term.watching}</span></div>
    <div class="l"><span class="gtag plugin">plugin</span><span class="gout">{$t.try.term.connecting}</span></div>
    <div class="l"><span class="gtag plugin">plugin</span><span class="gok">{$t.try.term.connected}</span> <span class="gout">{$t.try.term.connectedNote}</span></div>
    <div class="blank"></div>
    <div class="l"><span class="gout gcm">{$t.try.term.editDisk}</span></div>
    <div class="l"><span class="gtag daemon">daemon</span><span class="gout">{$t.try.term.appliedFs} <b>PlayerData</b> <span class="gok">{$t.try.term.applied}</span></span></div>
    <div class="l"><span class="gout gcm">{$t.try.term.editStudio}</span></div>
    <div class="l"><span class="gtag daemon">daemon</span><span class="gout">{$t.try.term.appliedStudioPre} <b>Shop</b> <span class="gmg">{$t.try.term.mergeLabel}</span> <span class="gok">{$t.try.term.mergeResult}</span></span></div>
    <div class="blank"></div>
    <div class="l"><span class="gok">●</span> <span class="gout">{$t.try.term.done}<span class="logcur"></span></span></div>
  </div>
</div>

<style>
  .win { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; font-family: var(--mono); box-shadow: 0 40px 90px -55px rgba(124,131,255,.4), 0 30px 60px -50px #000; }
  .tbar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .ttl { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .gterm { padding: 18px 20px; font-size: .82rem; line-height: 1.85; background: #08090d; }
  .l { white-space: pre-wrap; }
  .gp { color: var(--fs); } .gcmd { color: var(--text); } .gout { color: var(--text-dim); } .gout b { color: var(--text); font-weight: 500; } .gcm { color: #5b6472; }
  .gok { color: var(--ok); } .gmg { color: var(--merge); }
  .gtag { display: inline-block; font-size: .64rem; padding: 1px 7px; border-radius: 5px; margin-right: 8px; }
  .gtag.plugin { background: rgba(92,200,255,.14); color: var(--studio); }
  .gtag.daemon { background: rgba(154,160,255,.14); color: var(--merge); }
  .blank { height: 8px; }
  .logcur { display: inline-block; width: 8px; height: 14px; background: var(--ok); vertical-align: -2px; margin-left: 4px; animation: blink 1s steps(2) infinite; }
  @keyframes blink { 50% { opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .logcur { animation: none; } }
</style>
```

- [ ] **Step 2: Verify build**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add site/src/lib/GuidedTerminal.svelte
git commit -m "feat(site): guided terminal session (block 06)"
```

---

## Task 14: Rebuild App.svelte (assemble all blocks + i18n)

**Files:**
- Modify: `site/src/App.svelte` (full replacement)

- [ ] **Step 1: Replace App.svelte**

Overwrite `site/src/App.svelte`:

```svelte
<script>
  import Icon from './lib/Icon.svelte'
  import { reveal } from './lib/reveal.js'
  import { t } from './i18n/index.js'
  import logo from './assets/logo.png'
  import Hero from './lib/Hero.svelte'
  import LanguageToggle from './lib/LanguageToggle.svelte'
  import FeatureRow from './lib/FeatureRow.svelte'
  import ComparisonMatrix from './lib/ComparisonMatrix.svelte'
  import MergeEditor from './lib/MergeEditor.svelte'
  import StateInspector from './lib/StateInspector.svelte'
  import LogStream from './lib/LogStream.svelte'
  import WorkspaceTree from './lib/WorkspaceTree.svelte'
  import GuidedTerminal from './lib/GuidedTerminal.svelte'

  const REPO = 'https://github.com/vskstudio/naht'
</script>

<nav>
  <div class="wrap nav-inner">
    <a class="brand" href="#top"><img class="brand-mark" src={logo} alt="" width="26" height="26" /> naht</a>
    <div class="nav-links">
      <a href="#comparison">{$t.nav.comparison}</a>
      <a href="#merge">{$t.nav.merge}</a>
      <a href="#architecture">{$t.nav.architecture}</a>
      <a href="#start">{$t.nav.quickstart}</a>
      <LanguageToggle />
      <a class="nav-cta" href={REPO} target="_blank" rel="noreferrer"><Icon name="git" size={16} /> {$t.nav.github}</a>
    </div>
  </div>
</nav>

<Hero />

<main>
  <!-- 01 Comparison -->
  <section id="comparison">
    <div class="wrap">
      <div class="idx reveal" use:reveal><span class="n">{$t.matrix.num}</span> {$t.matrix.label} <span class="bar"></span></div>
      <h2 class="section-title reveal" use:reveal>{$t.matrix.title}</h2>
      <p class="section-lead reveal" use:reveal>{$t.matrix.lead}</p>
      <div class="reveal" use:reveal={{ delay: 80 }}><ComparisonMatrix /></div>
    </div>
  </section>

  <!-- 02 Merge -->
  <section id="merge" class="alt">
    <div class="wrap">
      <div class="idx reveal" use:reveal><span class="n">{$t.merge.num}</span> Merge 3-way <span class="bar"></span></div>
      <span class="ftag m reveal" use:reveal>{$t.merge.tag}</span>
      <h2 class="section-title reveal narrow" use:reveal>{$t.merge.title}</h2>
      <p class="section-lead reveal" use:reveal>{$t.merge.lead} <span class="muted">{$t.merge.leadVs}</span></p>
      <div class="reveal" use:reveal={{ delay: 80 }}><MergeEditor /></div>
    </div>
  </section>

  <!-- 03 Base -->
  <FeatureRow id="base-persisted" num={$t.base.num} label={$t.nav.base ?? 'Base'} title={$t.base.title} tag={$t.base.tag} flip>
    <svelte:fragment slot="lead">{$t.base.lead} <span class="muted">{$t.base.leadVs}</span></svelte:fragment>
    <StateInspector />
  </FeatureRow>

  <!-- 04 Non-destructive -->
  <FeatureRow num={$t.nondestructive.num} label="Non-destructif" title={$t.nondestructive.title} tag={$t.nondestructive.tag} tagCls="o">
    <svelte:fragment slot="lead">{$t.nondestructive.lead} <span class="muted">{$t.nondestructive.leadVs}</span></svelte:fragment>
    <LogStream />
  </FeatureRow>

  <!-- 05 Architecture -->
  <FeatureRow id="architecture" num={$t.architecture.num} label="Architecture" title={$t.architecture.title} tag={$t.architecture.tag} tagCls="s" flip>
    <svelte:fragment slot="lead">{$t.architecture.lead} <span class="muted">{$t.architecture.leadVs}</span></svelte:fragment>
    <WorkspaceTree />
  </FeatureRow>

  <!-- 06 Try -->
  <section id="start" class="alt">
    <div class="wrap">
      <div class="idx reveal" use:reveal><span class="n">{$t.try.num}</span> {$t.try.label} <span class="bar"></span></div>
      <h2 class="section-title reveal" use:reveal>{$t.try.title}</h2>
      <p class="section-lead reveal" use:reveal>{$t.try.lead}</p>
      <div class="reveal" use:reveal={{ delay: 80 }}><GuidedTerminal /></div>

      <div class="limits-note reveal" use:reveal>
        <h4>{$t.try.limitsTitle}</h4>
        {#each $t.try.limits as l (l.label)}
          <span class="lpill">{l.label} <span class="s {l.cls}">{l.status}</span></span>
        {/each}
      </div>

      <div class="start-cta reveal" use:reveal>
        <a class="btn primary" href={REPO + '/blob/main/docs/quickstart.md'} target="_blank" rel="noreferrer"><Icon name="arrow" size={17} /> {$t.try.ctaPrimary}</a>
        <a class="btn ghost" href={REPO + '/blob/main/docs/architecture.md'} target="_blank" rel="noreferrer"><Icon name="layers" size={17} /> {$t.try.ctaSecondary}</a>
      </div>
    </div>
  </section>
</main>

<footer>
  <div class="wrap foot-inner">
    <div class="foot-brand">
      <img class="brand-mark" src={logo} alt="" width="30" height="30" />
      <div><strong>naht</strong><span>{$t.footer.tagline}</span></div>
    </div>
    <div class="foot-links">
      <a href={REPO} target="_blank" rel="noreferrer">{$t.footer.links.github}</a>
      <a href={REPO + '/blob/main/docs/quickstart.md'} target="_blank" rel="noreferrer">{$t.footer.links.quickstart}</a>
      <a href={REPO + '/blob/main/docs/architecture.md'} target="_blank" rel="noreferrer">{$t.footer.links.architecture}</a>
      <a href={REPO + '/blob/main/docs/prior-art.md'} target="_blank" rel="noreferrer">{$t.footer.links.priorArt}</a>
    </div>
    <div class="foot-license">{$t.footer.license}</div>
  </div>
</footer>

<style>
  nav { position: sticky; top: 0; z-index: 50; backdrop-filter: blur(12px); background: rgba(7,8,9,.72); border-bottom: 1px solid var(--border-soft); }
  .nav-inner { display: flex; align-items: center; justify-content: space-between; height: 62px; }
  .brand { display: inline-flex; align-items: center; gap: 9px; font-family: var(--mono); font-weight: 700; font-size: 1.1rem; color: var(--text); }
  .brand-mark { display: block; width: 26px; height: 26px; image-rendering: pixelated; }
  .nav-links { display: flex; align-items: center; gap: 22px; }
  .nav-links a { color: var(--text-dim); font-size: 0.9rem; }
  .nav-links a:hover { color: var(--text); }
  .nav-cta { display: inline-flex; align-items: center; gap: 7px; padding: 7px 14px; border: 1px solid var(--border); border-radius: 9px; color: var(--text) !important; }
  .nav-cta:hover { border-color: var(--fs); }
  @media (max-width: 820px) { .nav-links a:not(.nav-cta) { display: none; } }

  section.alt { background: var(--bg-soft); border-top: 1px solid var(--border-soft); border-bottom: 1px solid var(--border-soft); }
  .idx { display: flex; align-items: center; gap: 12px; font-family: var(--mono); font-size: .72rem; letter-spacing: .16em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 18px; }
  .idx .n { color: var(--fs); } .idx .bar { flex: 1; height: 1px; max-width: 64px; background: var(--border-strong); }
  .ftag { display: inline-flex; align-items: center; gap: 8px; font-family: var(--mono); font-size: .72rem; letter-spacing: .04em; color: var(--fs); background: rgba(245,181,74,.1); border: 1px solid rgba(245,181,74,.22); padding: 4px 11px; border-radius: 999px; margin-bottom: 16px; }
  .ftag.m { color: var(--merge); background: rgba(154,160,255,.1); border-color: rgba(154,160,255,.22); }
  .section-title.narrow { max-width: 28ch; }
  .section-lead .muted { color: var(--text-faint); }

  .limits-note { margin-top: 34px; border: 1px solid var(--border); border-radius: 13px; background: var(--bg-card); padding: 18px 20px; }
  .limits-note h4 { margin: 0 0 12px; font-size: .82rem; font-family: var(--mono); color: var(--text-faint); font-weight: 600; letter-spacing: .05em; text-transform: uppercase; }
  .lpill { display: inline-flex; align-items: center; gap: 7px; margin: 0 8px 8px 0; font-family: var(--mono); font-size: .72rem; padding: 5px 11px; border-radius: 8px; border: 1px solid var(--border-soft); background: #0a0b0e; color: var(--text-dim); }
  .lpill .s { font-size: .62rem; padding: 2px 6px; border-radius: 5px; }
  .lpill .s.ok { background: rgba(74,210,149,.14); color: var(--ok); }
  .lpill .s.wn { background: rgba(245,181,74,.14); color: var(--fs); }
  .lpill .s.bd { background: rgba(242,97,106,.14); color: var(--danger); }

  .start-cta { display: flex; gap: 13px; flex-wrap: wrap; margin-top: 34px; }

  footer { border-top: 1px solid var(--border); padding: 40px 0; margin-top: 40px; }
  .foot-inner { display: flex; flex-wrap: wrap; gap: 24px; align-items: center; justify-content: space-between; }
  .foot-brand { display: flex; align-items: center; gap: 12px; }
  .foot-brand strong { font-family: var(--mono); display: block; }
  .foot-brand span { color: var(--text-dim); font-size: 0.85rem; }
  .foot-brand .brand-mark { width: 30px; height: 30px; }
  .foot-links { display: flex; gap: 18px; }
  .foot-links a { color: var(--text-dim); font-size: 0.9rem; }
  .foot-links a:hover { color: var(--text); }
  .foot-license { color: var(--text-faint); font-size: 0.82rem; }
</style>
```

Note: `label={$t.nav.base ?? 'Base'}` — block 03's label. Add a `base: 'Base'` entry to `nav` in BOTH dictionaries to avoid the fallback (do it now: add `base: 'Base'` to `nav` in `en.js` and `fr.js`; the parity test from Task 3 must still pass — it's the same key in both). Likewise the literal labels `'Non-destructif'` (block 04) and `'Architecture'` (block 05) and `'Merge 3-way'` (block 02 idx) are acceptable as-is since they read identically in FR/EN, but for full i18n add `nav.nondestructive: 'Non-destructive'/'Non-destructif'` and use it. Apply this in Step 2.

- [ ] **Step 2: Add the missing nav labels and use them**

In `en.js` `nav`, add: `base: 'Base',` and `nondestructive: 'Non-destructive',`.
In `fr.js` `nav`, add: `base: 'Base',` and `nondestructive: 'Non-destructif',`.
In `App.svelte`, change block 03 to `label={$t.nav.base}` and block 04 to `label={$t.nav.nondestructive}`.

- [ ] **Step 3: Run the parity test**

Run: `cd site && npx vitest run`
Expected: PASS (en/fr still symmetric).

- [ ] **Step 4: Build**

Run: `cd site && npm run build`
Expected: succeeds, no unresolved imports.

- [ ] **Step 5: Commit**

```bash
git add site/src/App.svelte site/src/i18n/en.js site/src/i18n/fr.js
git commit -m "feat(site): assemble redesigned landing with 7 blocks + i18n nav"
```

---

## Task 15: Remove dead components

**Files:**
- Delete: `site/src/lib/DataFlowDiagram.svelte`, `site/src/lib/MergeDiagram.svelte`, `site/src/lib/ArchitectureDiagram.svelte`, `site/src/lib/BentoGrid.svelte`

- [ ] **Step 1: Confirm nothing imports them**

Run: `cd site && grep -rn "DataFlowDiagram\|MergeDiagram\|ArchitectureDiagram\|BentoGrid" src`
Expected: no matches (Card may still exist and is kept — do not delete `Card.svelte`).

- [ ] **Step 2: Delete the files**

Run:
```bash
cd site && rm src/lib/DataFlowDiagram.svelte src/lib/MergeDiagram.svelte src/lib/ArchitectureDiagram.svelte src/lib/BentoGrid.svelte
```

- [ ] **Step 3: Build to confirm nothing broke**

Run: `cd site && npm run build`
Expected: succeeds.

- [ ] **Step 4: Commit**

```bash
git add -A site/src/lib
git commit -m "chore(site): remove components superseded by v2 artifacts"
```

---

## Task 16: Final verification + manual QA

**Files:** none (verification only)

- [ ] **Step 1: Full test suite**

Run: `cd site && npx vitest run`
Expected: all i18n + dictionary tests pass.

- [ ] **Step 2: Production build**

Run: `cd site && npm run build`
Expected: succeeds with no warnings about unresolved imports or missing exports.

- [ ] **Step 3: Manual QA in the dev server**

Run: `cd site && npm run dev`, open the printed localhost URL, and verify against `fullpage-v8.html`:
- All 7 blocks render in order: hero seam → 01 matrix → 02 merge editor → 03 SQLite inspector → 04 log stream → 05 workspace+tests → 06 guided terminal + limits pills + CTAs → footer.
- Hero seam: panes float (no outer block), chips ride the curves, status line below.
- Merge editor is full-width; conflict markers + auto-merged hunk styled.
- Matrix: Naht column has the gradient top border + amber tint; rojo/argon cells colored per status.
- Clicking **FR / EN** in the nav swaps every visible string — including artifact contents (log lines, table footer, terminal prose, badges) — with no layout break.
- Reload the page: the chosen language persists (localStorage), and `<html lang>` matches (check DevTools elements panel).
- First-visit detection: in a fresh profile / cleared storage with browser language set to French, the page loads in FR; otherwise EN.
- With OS "reduce motion" on: seam chips, log/terminal cursors, and reveal-on-scroll do not animate.
- No console errors.

- [ ] **Step 4: Final commit (if any QA tweaks were needed)**

```bash
git add -A site
git commit -m "fix(site): redesign v2 QA pass"
```

(Skip if Step 3 surfaced nothing to change.)

---

## Self-review notes (author)

- **Spec coverage:** hero seam (Task 5/6), 01 matrix (Task 8), 02 merge editor (Task 9), 03 SQLite (Task 10), 04 log stream (Task 11), 05 workspace+tests (Task 12), 06 guided terminal + limits + CTAs (Task 13/14), footer (Task 14), i18n core + dicts + toggle + `<html lang>` + persistence + detection + default `en` (Tasks 2-4, 14), maxw 1440 (Task 1), reduced-motion (Tasks 5/11/13 + existing app.css), build gate (Task 16). All spec sections map to a task.
- **Type/name consistency:** dictionary keys referenced by components (`$t.matrix.rows[].rojoCls`, `$t.merge.footerClean`, `$t.nondestructive.logs[].lv`, `$t.architecture.tomlComment`, `$t.try.term.*`, `$t.try.limits[].cls`) all match the dictionary definitions in Task 3 + the additions in Task 14. `setLocale`/`locale`/`t`/`LOCALES` names are consistent across Tasks 2, 4, 14.
- **Placeholders:** none — every component and dictionary is given in full.
