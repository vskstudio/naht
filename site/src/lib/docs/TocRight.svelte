<script>
  import { onMount } from 'svelte'
  export let toc = []
  let activeId = ''

  onMount(() => {
    let observer
    const wire = () => {
      observer?.disconnect()
      const targets = toc.map((t) => document.getElementById(t.id)).filter(Boolean)
      if (!targets.length) return
      observer = new IntersectionObserver(
        (entries) => {
          for (const e of entries) if (e.isIntersecting) activeId = e.target.id
        },
        { rootMargin: '0px 0px -70% 0px', threshold: 0 },
      )
      targets.forEach((el) => observer.observe(el))
    }
    wire()
    return () => observer?.disconnect()
  })

  // Re-wire when the toc changes (doc switched).
  $: toc, typeof document !== 'undefined' && queueMicrotask(() => {
    activeId = ''
  })
</script>

{#if toc.length}
  <nav class="toc">
    <div class="toc-title">On this page</div>
    {#each toc as t}
      <a class="toc-link" class:sub={t.level === 3} class:active={t.id === activeId} href={`#${t.id}`}>{t.text}</a>
    {/each}
  </nav>
{/if}

<style>
  .toc { position: sticky; top: 76px; }
  .toc-title { font-family: var(--mono); font-size: 0.72rem; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 10px; }
  .toc-link { display: block; padding: 3px 0; color: var(--text-dim); font-size: 0.85rem; border-left: 2px solid transparent; padding-left: 12px; }
  .toc-link.sub { padding-left: 24px; }
  .toc-link:hover { color: var(--text); }
  .toc-link.active { color: var(--fs); border-left-color: var(--fs); }
</style>
