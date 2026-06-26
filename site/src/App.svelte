<script>
  import Icon from './lib/Icon.svelte'
  import { reveal } from './lib/reveal.js'
  import Hero from './lib/Hero.svelte'
  import DataFlowDiagram from './lib/DataFlowDiagram.svelte'
  import MergeDiagram from './lib/MergeDiagram.svelte'
  import ArchitectureDiagram from './lib/ArchitectureDiagram.svelte'
  import logo from './assets/logo.png'

  const REPO = 'https://github.com/vskstudio/naht'

  const pains = [
    {
      pain: 'Two-way sync is experimental — it deletes Studio edits or crashes the server.',
      fix: 'Bidirectional is the core design. No unwrap() in the sync loop — a failed write pauses one path, never kills the session.',
      icon: 'sync',
    },
    {
      pain: 'Overwrite-on-conflict, no merge.',
      fix: 'A real 3-way text merge against a persisted base; unmergeable hunks get git-style markers and freeze the path until resolved.',
      icon: 'merge',
    },
    {
      pain: 'Reconciliation state is in memory and lost on restart.',
      fix: 'The last-sync base is persisted to SQLite, so restarts and reconnects re-diff safely instead of re-clobbering.',
      icon: 'database',
    },
    {
      pain: 'Verbose default.project.json + scattered .meta.json sidecars.',
      fix: 'Convention over configuration, layered config, inline property frontmatter. naht init --from-rojo migrates an existing project tree.',
      icon: 'layers',
    },
    {
      pain: 'Live-sync gaps fail silently.',
      fix: 'Unsyncable properties (CSG, terrain, MeshId, locked props) are detected and reported with guidance, never dropped.',
      icon: 'shield',
    },
    {
      pain: 'Connection death is silent or fatal.',
      fix: 'Heartbeat, auto-reconnect with backoff, a visible connection state, and a re-diff against persisted state on reconnect.',
      icon: 'plug',
    },
  ]

  const commands = [
    { cmd: 'naht init [path]', desc: 'Scaffold a project. --from-rojo converts a default.project.json.' },
    { cmd: 'naht serve [path]', desc: 'Run the localhost sync daemon. --port to override, -v/-vv for logs.' },
    { cmd: 'naht status [path]', desc: 'List paths frozen by a conflict.' },
    { cmd: 'naht resolve <path>', desc: 'Clear a conflict once its markers are gone.' },
    { cmd: 'naht build [path] -o out.rbxm', desc: 'Build a model or place file. --watch to rebuild on change.' },
    { cmd: 'naht pull [path]', desc: 'Ask a running daemon to re-sync now.' },
    { cmd: 'naht package-plugin -o naht-plugin.rbxmx', desc: 'Package the Studio plugin into an installable model.' },
  ]

  const limits = [
    { case: 'MeshId / images', status: 'upload', cls: 'ok', note: 'Local mesh/image uploaded once via Open Cloud, cached by content hash, reference rewritten to rbxassetid://. Off by default.' },
    { case: 'Terrain', status: 'syncable', cls: 'ok', note: 'Read/written via ReadVoxels/WriteVoxels as an opaque voxel blob; hash-compared, both-sides change freezes.' },
    { case: 'CSG / Unions', status: 'round-trip', cls: 'warn', note: 'Engine-generated binary geometry round-trips inside rbxm model files — opaque, file-level.' },
    { case: 'HttpEnabled & locked props', status: 'hard block', cls: 'bad', note: 'Not settable by scripts/plugins by design. Naht warns and points to Game Settings; offers a place-file fallback.' },
  ]

  const steps = [
    { n: 1, title: 'Install the CLI', body: 'Download the binary for your platform from Releases, or cargo build --release -p naht.' },
    { n: 2, title: 'Install the plugin', body: 'Each release ships naht-plugin.rbxmx. Insert it in Studio and allow HTTP requests.' },
    { n: 3, title: 'Create a project', body: 'naht init demo scaffolds src/, a minimal naht.toml, and a .gitignore for .naht/.' },
    { n: 4, title: 'Start the daemon', body: 'naht serve watches the filesystem and serves on http://localhost:34872.' },
    { n: 5, title: 'Connect Studio', body: 'Click Naht Sync. The widget walks Connecting → Connected; source appears under ServerStorage/Naht.' },
    { n: 6, title: 'Edit on both sides', body: 'Change a file on disk or a ModuleScript in Studio — the other side follows, no manual push or pull.' },
  ]
</script>

<!-- Sticky top nav -->
<nav>
  <div class="wrap nav-inner">
    <a class="brand" href="#top">
      <img class="brand-mark" src={logo} alt="" width="26" height="26" /> naht
    </a>
    <div class="nav-links">
      <a href="#why">Why</a>
      <a href="#architecture">Architecture</a>
      <a href="#sync">Sync</a>
      <a href="#cli">CLI</a>
      <a href="#start">Quickstart</a>
      <a class="nav-cta" href={REPO} target="_blank" rel="noreferrer">
        <Icon name="git" size={16} /> GitHub
      </a>
    </div>
  </div>
</nav>

<Hero />

<main>
  <!-- Why Naht -->
  <section id="why">
    <div class="wrap">
      <div class="eyebrow reveal" use:reveal><Icon name="warn" size={14} /> Why Naht</div>
      <h2 class="section-title reveal" use:reveal>Built around the failure modes that make the others painful.</h2>
      <p class="section-lead reveal" use:reveal>
        A from-scratch alternative to Rojo and Argon. Every decision is grounded in a concrete failure
        of the incumbents — see the prior-art teardown in the repo.
      </p>
      <div class="pain-grid">
        {#each pains as p, i (p.pain)}
          <div class="pain-card reveal" use:reveal={{ delay: (i % 3) * 90 }}>
            <span class="pain-icon"><Icon name={p.icon} size={20} /></span>
            <div class="pain-bad"><Icon name="warn" size={14} /> {p.pain}</div>
            <div class="pain-fix"><Icon name="check" size={14} /> {p.fix}</div>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- Architecture -->
  <section id="architecture" class="alt">
    <div class="wrap">
      <div class="eyebrow reveal" use:reveal><Icon name="layers" size={14} /> Architecture</div>
      <h2 class="section-title reveal" use:reveal>A Cargo workspace and a thin Luau plugin.</h2>
      <p class="section-lead reveal" use:reveal>
        The brain has zero network I/O so it stays unit-testable; the daemon owns transport and the
        disk; the plugin is kept deliberately thin — the thinner the plugin, the fewer the bugs.
      </p>
      <ArchitectureDiagram />
    </div>
  </section>

  <!-- Sync / data flow -->
  <section id="sync">
    <div class="wrap">
      <div class="eyebrow reveal" use:reveal><Icon name="sync" size={14} /> The seam, working</div>
      <h2 class="section-title reveal" use:reveal>Bidirectional sync with a persisted base.</h2>
      <p class="section-lead reveal" use:reveal>
        The filesystem is read fresh on every reconcile; the Studio side is mirrored in memory.
        Filesystem → Studio patches are <b>ack-gated</b> — the base advances only once the plugin
        confirms it applied, so a half-applied batch re-diffs the rest instead of clobbering.
      </p>
      <div class="reveal" use:reveal><DataFlowDiagram /></div>

      <h3 class="sub reveal" use:reveal><Icon name="merge" size={18} /> When both sides change the same script</h3>
      <p class="section-lead reveal" use:reveal>
        A 3-way merge against the last-sync content. Clean merges are written and the base advances;
        an unmergeable hunk freezes the path and never auto-picks a winner.
      </p>
      <div class="reveal" use:reveal><MergeDiagram /></div>
    </div>
  </section>

  <!-- CLI -->
  <section id="cli" class="alt">
    <div class="wrap">
      <div class="eyebrow reveal" use:reveal><Icon name="terminal" size={14} /> Command line</div>
      <h2 class="section-title reveal" use:reveal>Convention-first. The config carries only exceptions.</h2>
      <p class="section-lead reveal" use:reveal>
        An optional <code>naht.toml</code> (layered over <code>~/.naht/config.toml</code>) holds just
        the project name, the serve port, and the place-id guard.
      </p>
      <div class="cmd-grid">
        {#each commands as c, i (c.cmd)}
          <div class="cmd-card reveal" use:reveal={{ delay: (i % 2) * 80 }}>
            <code><span class="prompt">$</span> {c.cmd}</code>
            <p>{c.desc}</p>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- Limits -->
  <section id="limits">
    <div class="wrap">
      <div class="eyebrow reveal" use:reveal><Icon name="shield" size={14} /> Limits, handled honestly</div>
      <h2 class="section-title reveal" use:reveal>The API ceiling is real — Naht reports it, never drops it.</h2>
      <p class="section-lead reveal" use:reveal>
        Studio plugins are Luau-only with no filesystem access and request/response HTTP. Where a value
        can't round-trip live, Naht detects and explains it instead of failing silently.
      </p>
      <div class="limits">
        {#each limits as l, i (l.case)}
          <div class="limit-row reveal" use:reveal={{ delay: i * 70 }}>
            <div class="limit-case">{l.case}</div>
            <div class="limit-status"><span class="status {l.cls}">{l.status}</span></div>
            <div class="limit-note">{l.note}</div>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- Quickstart -->
  <section id="start" class="alt">
    <div class="wrap">
      <div class="eyebrow reveal" use:reveal><Icon name="bolt" size={14} /> Quickstart</div>
      <h2 class="section-title reveal" use:reveal>Zero to a confirmed bidirectional sync.</h2>
      <p class="section-lead reveal" use:reveal>Six steps from an empty folder to the seam working in both directions.</p>
      <div class="steps">
        {#each steps as s, i (s.n)}
          <div class="step reveal" use:reveal={{ delay: (i % 3) * 90 }}>
            <span class="step-n">{s.n}</span>
            <div>
              <strong>{s.title}</strong>
              <p>{s.body}</p>
            </div>
          </div>
        {/each}
      </div>
      <div class="start-cta reveal" use:reveal>
        <a class="btn primary" href={REPO + '/blob/main/docs/quickstart.md'} target="_blank" rel="noreferrer">
          <Icon name="arrow" size={17} /> Full quickstart
        </a>
        <a class="btn ghost" href={REPO + '/blob/main/docs/architecture.md'} target="_blank" rel="noreferrer">
          <Icon name="layers" size={17} /> Architecture doc
        </a>
      </div>
    </div>
  </section>
</main>

<footer>
  <div class="wrap foot-inner">
    <div class="foot-brand">
      <img class="brand-mark" src={logo} alt="" width="30" height="30" />
      <div>
        <strong>naht</strong>
        <span>The seam between your filesystem and Roblox Studio.</span>
      </div>
    </div>
    <div class="foot-links">
      <a href={REPO} target="_blank" rel="noreferrer">GitHub</a>
      <a href={REPO + '/blob/main/docs/quickstart.md'} target="_blank" rel="noreferrer">Quickstart</a>
      <a href={REPO + '/blob/main/docs/architecture.md'} target="_blank" rel="noreferrer">Architecture</a>
      <a href={REPO + '/blob/main/docs/prior-art.md'} target="_blank" rel="noreferrer">Prior art</a>
    </div>
    <div class="foot-license">Dual-licensed under MIT or Apache-2.0.</div>
  </div>
</footer>

<style>
  /* ---- Nav ---- */
  nav {
    position: sticky;
    top: 0;
    z-index: 50;
    backdrop-filter: blur(12px);
    background: rgba(11, 14, 20, 0.72);
    border-bottom: 1px solid var(--border-soft);
  }
  .nav-inner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 62px;
  }
  .brand {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    font-family: var(--mono);
    font-weight: 700;
    font-size: 1.1rem;
    color: var(--text);
  }
  .brand-mark {
    display: block;
    width: 26px;
    height: 26px;
    image-rendering: pixelated;
  }
  .nav-links {
    display: flex;
    align-items: center;
    gap: 26px;
  }
  .nav-links a {
    color: var(--text-dim);
    font-size: 0.9rem;
  }
  .nav-links a:hover {
    color: var(--text);
  }
  .nav-cta {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 7px 14px;
    border: 1px solid var(--border);
    border-radius: 9px;
    color: var(--text) !important;
  }
  .nav-cta:hover {
    border-color: var(--fs);
  }
  @media (max-width: 720px) {
    .nav-links a:not(.nav-cta) {
      display: none;
    }
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    padding: 12px 22px;
    border-radius: 11px;
    font-weight: 600;
    font-size: 0.95rem;
    transition:
      transform 0.15s ease,
      box-shadow 0.2s ease,
      background 0.2s ease;
  }
  .btn:hover {
    transform: translateY(-2px);
  }
  .btn.primary {
    background: linear-gradient(120deg, var(--fs), #f0a030);
    color: #1a1205;
    box-shadow: 0 10px 30px -10px rgba(245, 181, 74, 0.6);
  }
  .btn.ghost {
    color: var(--text);
    border: 1px solid var(--border);
    background: var(--bg-soft);
  }
  .btn.ghost:hover {
    border-color: var(--studio);
  }

  /* ---- Section chrome ---- */
  section.alt {
    background: var(--bg-soft);
    border-top: 1px solid var(--border-soft);
    border-bottom: 1px solid var(--border-soft);
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 1.3rem;
    margin: 64px 0 12px;
    color: var(--text);
  }
  .sub :global(svg) {
    color: var(--merge);
  }

  /* ---- Why grid ---- */
  .pain-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 18px;
  }
  .pain-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 22px;
    display: flex;
    flex-direction: column;
    gap: 13px;
    transition:
      transform 0.2s ease,
      border-color 0.2s ease;
  }
  .pain-card:hover {
    transform: translateY(-3px);
    border-color: var(--border);
  }
  .pain-icon {
    width: 44px;
    height: 44px;
    border-radius: 11px;
    display: grid;
    place-items: center;
    color: var(--fs);
    background: var(--fs-soft);
  }
  .pain-bad {
    display: flex;
    gap: 9px;
    font-size: 0.9rem;
    color: var(--text-dim);
  }
  .pain-bad :global(svg) {
    color: var(--danger);
    flex-shrink: 0;
    margin-top: 3px;
  }
  .pain-fix {
    display: flex;
    gap: 9px;
    font-size: 0.92rem;
    color: var(--text);
  }
  .pain-fix :global(svg) {
    color: var(--ok);
    flex-shrink: 0;
    margin-top: 3px;
  }

  /* ---- CLI grid ---- */
  .cmd-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  .cmd-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 18px 20px;
    transition: border-color 0.2s ease;
  }
  .cmd-card:hover {
    border-color: var(--fs);
  }
  .cmd-card code {
    display: block;
    font-size: 0.86rem;
    color: var(--text);
    margin-bottom: 7px;
    word-break: break-word;
  }
  .cmd-card .prompt {
    color: var(--fs);
    margin-right: 8px;
  }
  .cmd-card p {
    margin: 0;
    font-size: 0.86rem;
    color: var(--text-dim);
  }

  /* ---- Limits ---- */
  .limits {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .limit-row {
    display: grid;
    grid-template-columns: 1.1fr 0.7fr 3fr;
    gap: 16px;
    align-items: center;
    padding: 18px 22px;
    background: var(--bg-card);
    border-bottom: 1px solid var(--border-soft);
  }
  .limit-row:last-child {
    border-bottom: none;
  }
  .limit-case {
    font-family: var(--mono);
    font-size: 0.88rem;
    color: var(--text);
  }
  .status {
    font-family: var(--mono);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 4px 10px;
    border-radius: 999px;
    white-space: nowrap;
  }
  .status.ok {
    color: var(--ok);
    background: rgba(74, 210, 149, 0.12);
  }
  .status.warn {
    color: var(--fs);
    background: var(--fs-soft);
  }
  .status.bad {
    color: var(--danger);
    background: rgba(242, 97, 106, 0.12);
  }
  .limit-note {
    font-size: 0.88rem;
    color: var(--text-dim);
  }

  /* ---- Steps ---- */
  .steps {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 18px;
    margin-bottom: 40px;
  }
  .step {
    display: flex;
    gap: 15px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 20px;
  }
  .step-n {
    flex-shrink: 0;
    width: 34px;
    height: 34px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    font-family: var(--mono);
    font-weight: 700;
    color: var(--bg);
    background: linear-gradient(120deg, var(--fs), var(--merge));
  }
  .step strong {
    display: block;
    margin-bottom: 5px;
    font-size: 0.98rem;
  }
  .step p {
    margin: 0;
    font-size: 0.86rem;
    color: var(--text-dim);
  }
  .start-cta {
    display: flex;
    gap: 13px;
    flex-wrap: wrap;
  }

  /* ---- Footer ---- */
  footer {
    border-top: 1px solid var(--border-soft);
    padding: 46px 0;
  }
  .foot-inner {
    display: flex;
    flex-wrap: wrap;
    gap: 22px;
    align-items: center;
    justify-content: space-between;
  }
  .foot-brand {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .foot-brand strong {
    font-family: var(--mono);
    display: block;
  }
  .foot-brand span {
    font-size: 0.84rem;
    color: var(--text-faint);
  }
  .foot-links {
    display: flex;
    gap: 20px;
    flex-wrap: wrap;
  }
  .foot-links a {
    color: var(--text-dim);
    font-size: 0.9rem;
  }
  .foot-license {
    font-size: 0.8rem;
    color: var(--text-faint);
    width: 100%;
  }

  @media (max-width: 860px) {
    .pain-grid,
    .steps {
      grid-template-columns: 1fr 1fr;
    }
  }
  @media (max-width: 640px) {
    .pain-grid,
    .steps,
    .cmd-grid {
      grid-template-columns: 1fr;
    }
    .limit-row {
      grid-template-columns: 1fr;
      gap: 8px;
    }
  }
</style>
