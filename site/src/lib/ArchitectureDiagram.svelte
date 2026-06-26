<script>
  import Icon from './Icon.svelte'
  import { reveal } from './reveal.js'

  const layers = [
    {
      key: 'core',
      icon: 'merge',
      title: 'naht-core',
      tag: 'library · zero network I/O',
      blurb: 'The brain: VFS, PathMapper, middleware over rbx-dom, reconciler, 3-way merge, SQLite base, protocol types.',
      bullets: ['Typed errors (thiserror)', 'Unit-testable over an in-memory VFS'],
    },
    {
      key: 'daemon',
      icon: 'terminal',
      title: 'naht',
      tag: 'binary · CLI + localhost daemon',
      blurb: 'axum HTTP server, notify file watcher, session orchestration, SQLite ownership, every CLI command.',
      bullets: ['anyhow at the boundary', 'Binds 127.0.0.1 only'],
    },
    {
      key: 'plugin',
      icon: 'plug',
      title: 'plugin/',
      tag: 'Luau · deliberately thin',
      blurb: 'Long-polls the daemon, applies patches to the DataModel, posts Studio edits back, renders connection state.',
      bullets: ['No sync logic lives here', 'MessagePack codec, headless-tested'],
    },
  ]
</script>

<div class="arch">
  {#each layers as layer, i (layer.key)}
    <div class="layer {layer.key} reveal" use:reveal={{ delay: i * 110 }}>
      <div class="lh">
        <span class="lbadge {layer.key}"><Icon name={layer.icon} size={20} /></span>
        <div>
          <strong>{layer.title}</strong>
          <span class="ltag">{layer.tag}</span>
        </div>
      </div>
      <p>{layer.blurb}</p>
      <ul>
        {#each layer.bullets as b (b)}
          <li><Icon name="check" size={14} /> {b}</li>
        {/each}
      </ul>
    </div>

    {#if i === 1}
      <div class="boundary reveal" use:reveal>
        <span class="line"></span>
        <span class="boundary-label"><Icon name="bolt" size={14} /> HTTP · localhost · MessagePack</span>
        <span class="line"></span>
      </div>
    {:else if i === 0}
      <div class="connector reveal" use:reveal aria-hidden="true"><span></span></div>
    {/if}
  {/each}
</div>

<style>
  .arch {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 0;
  }
  .layer {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 20px 22px;
    background: var(--bg-card);
    position: relative;
  }
  .layer.core {
    border-left: 3px solid var(--merge);
  }
  .layer.daemon {
    border-left: 3px solid var(--fs);
  }
  .layer.plugin {
    border-left: 3px solid var(--studio);
  }

  .lh {
    display: flex;
    align-items: center;
    gap: 13px;
    margin-bottom: 9px;
  }
  .lbadge {
    width: 42px;
    height: 42px;
    border-radius: 11px;
    display: grid;
    place-items: center;
    flex-shrink: 0;
  }
  .lbadge.core {
    color: var(--merge);
    background: rgba(139, 124, 246, 0.13);
  }
  .lbadge.daemon {
    color: var(--fs);
    background: var(--fs-soft);
  }
  .lbadge.plugin {
    color: var(--studio);
    background: var(--studio-soft);
  }
  .lh strong {
    font-family: var(--mono);
    font-size: 1.05rem;
    display: block;
  }
  .ltag {
    font-size: 0.78rem;
    color: var(--text-faint);
  }
  .layer p {
    margin: 0 0 12px;
    color: var(--text-dim);
    font-size: 0.92rem;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 8px 18px;
  }
  li {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 0.82rem;
    color: var(--text);
  }
  li :global(svg) {
    color: var(--ok);
  }

  .connector {
    height: 26px;
    display: grid;
    place-items: center;
  }
  .connector span {
    width: 2px;
    height: 100%;
    background: linear-gradient(var(--merge), var(--fs));
    opacity: 0.5;
  }

  .boundary {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 4px;
  }
  .boundary .line {
    flex: 1;
    height: 1px;
    background: repeating-linear-gradient(90deg, var(--border) 0 6px, transparent 6px 12px);
  }
  .boundary-label {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-family: var(--mono);
    font-size: 0.74rem;
    color: var(--fs);
    white-space: nowrap;
    padding: 5px 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-soft);
  }
</style>
