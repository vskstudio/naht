<script>
  import { sections, docs } from './nav.js'
  export let slug
</script>

<aside class="sidebar">
  {#each sections as section}
    <div class="group">
      <div class="group-title">{section.title}</div>
      {#each section.items as item}
        <a class="link" class:active={item === slug} href={`#/docs/${item}`}>{docs[item].title}</a>
      {/each}
    </div>
  {/each}
</aside>

<style>
  .sidebar { padding: 26px 16px; }
  .group { margin-bottom: 22px; }
  .group-title {
    font-family: var(--mono); font-size: 0.72rem; letter-spacing: 0.06em; text-transform: uppercase;
    color: var(--text-faint); margin-bottom: 8px; padding: 0 10px;
  }
  .link { display: block; padding: 6px 10px; border-radius: 7px; color: var(--text-dim); font-size: 0.92rem; }
  .link:hover { color: var(--text); background: var(--bg-soft); }
  .link.active { color: var(--fs); background: var(--fs-soft); }
</style>
