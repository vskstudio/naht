<script>
  import { onMount } from 'svelte'
  import Icon from '../Icon.svelte'
  export let toc = []
  export let slug
  export let locale = 'en'
  let activeId = ''

  let observer
  function wire() {
    observer?.disconnect()
    const targets = toc.map((t) => document.getElementById(t.id)).filter(Boolean)
    if (!targets.length) return
    observer = new IntersectionObserver(
      (entries) => { for (const e of entries) if (e.isIntersecting) activeId = e.target.id },
      { rootMargin: '0px 0px -70% 0px', threshold: 0 },
    )
    targets.forEach((el) => observer.observe(el))
  }

  onMount(() => {
    wire()
    return () => observer?.disconnect()
  })

  // Re-wire when the toc changes (doc switched). queueMicrotask lets the new
  // headings render before we query them.
  $: toc, typeof document !== 'undefined' && queueMicrotask(() => {
    activeId = ''
    wire()
  })
</script>

{#if toc.length}
  <nav class="toc">
    <div class="toc-title"><Icon name="list" size={13} stroke={2.2} /> {locale === 'fr' ? 'Sur cette page' : 'On this page'}</div>
    {#each toc as t}
      <a class="toc-link" class:sub={t.level === 3} class:active={t.id === activeId} href={`#/docs/${slug}#${t.id}`}>{t.text}</a>
    {/each}
  </nav>
{/if}

<style>
  .toc { position: sticky; top: 76px; }
  .toc-title { display: flex; align-items: center; gap: 7px; font-family: var(--mono); font-size: 0.7rem; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 12px; }
  .toc-title :global(svg) { opacity: 0.6; }
  .toc-link { display: block; padding: 3px 0; color: var(--text-dim); font-size: 0.85rem; border-left: 2px solid transparent; padding-left: 12px; }
  .toc-link.sub { padding-left: 24px; }
  .toc-link:hover { color: var(--text); }
  .toc-link.active { color: var(--fs); border-left-color: var(--fs); }
</style>
