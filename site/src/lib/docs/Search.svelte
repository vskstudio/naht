<script>
  import { onMount } from 'svelte'
  import { buildSearchIndex } from './markdown.js'
  import { navigate } from './router.js'

  export let open = false
  let q = ''
  let sel = 0
  const index = buildSearchIndex()

  $: results = q.trim()
    ? index.filter((e) => e.text.toLowerCase().includes(q.trim().toLowerCase())).slice(0, 12)
    : index.filter((e) => e.heading === null)
  $: if (results) sel = 0

  function go(e) {
    if (!e) return
    navigate(e.id ? `#/docs/${e.slug}#${e.id}` : `#/docs/${e.slug}`)
    open = false
    q = ''
  }
  function onKey(ev) {
    if (!open) return
    if (ev.key === 'Escape') open = false
    else if (ev.key === 'ArrowDown') { if (!results.length) return; sel = Math.min(sel + 1, results.length - 1); ev.preventDefault() }
    else if (ev.key === 'ArrowUp') { sel = Math.max(sel - 1, 0); ev.preventDefault() }
    else if (ev.key === 'Enter') { go(results[sel]); ev.preventDefault() }
  }
  onMount(() => {
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })
</script>

{#if open}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <div class="overlay" on:click={() => (open = false)} role="presentation">
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-interactive-supports-focus -->
    <div class="modal" on:click|stopPropagation role="dialog" aria-label="Search docs" tabindex="-1">
      <!-- svelte-ignore a11y-autofocus -->
      <input autofocus placeholder="Search docs…" bind:value={q} />
      <ul>
        {#each results as r, i}
          <!-- svelte-ignore a11y-click-events-have-key-events -->
          <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
          <li class:sel={i === sel} on:mouseenter={() => (sel = i)} on:click={() => go(r)}>
            <span class="doc">{r.title}</span>{#if r.heading}<span class="sep">›</span><span class="head">{r.heading}</span>{/if}
          </li>
        {/each}
        {#if !results.length}<li class="empty">No matches</li>{/if}
      </ul>
    </div>
  </div>
{/if}

<style>
  .overlay { position: fixed; inset: 0; z-index: 60; background: rgba(0,0,0,0.55); display: flex; justify-content: center; align-items: flex-start; padding-top: 12vh; }
  .modal { width: min(560px, 92vw); background: var(--bg-card); border: 1px solid var(--border-strong); border-radius: var(--radius); box-shadow: var(--shadow); overflow: hidden; }
  input { width: 100%; padding: 16px 18px; background: none; border: none; border-bottom: 1px solid var(--border); color: var(--text); font-size: 1rem; outline: none; }
  ul { list-style: none; margin: 0; padding: 6px; max-height: 50vh; overflow-y: auto; }
  li { display: flex; align-items: center; gap: 8px; padding: 9px 12px; border-radius: 8px; color: var(--text-dim); cursor: pointer; font-size: 0.9rem; }
  li.sel { background: var(--bg-soft); color: var(--text); }
  .doc { color: var(--text); }
  .sep { color: var(--text-faint); }
  .head { color: var(--text-dim); }
  .empty { color: var(--text-faint); cursor: default; }
  @media (prefers-reduced-motion: reduce) { .modal { animation: none; } }
</style>
