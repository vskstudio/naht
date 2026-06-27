<script>
  import { sections, docs } from './nav.js'
  export let slug
  export let open = false
  export let onClose = () => {}
</script>

<aside class="sidebar" class:drawer-open={open}>
  {#each sections as section}
    <div class="group">
      <div class="group-title">{section.title}</div>
      {#each section.items as item}
        <a class="link" class:active={item === slug} href={`#/docs/${item}`} on:click={onClose}>{docs[item].title}</a>
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

  /* Mobile drawer: hidden by default, slides in when .drawer-open */
  @media (max-width: 720px) {
    .sidebar {
      position: fixed;
      top: 56px;
      left: 0;
      bottom: 0;
      width: min(280px, 85vw);
      background: var(--bg);
      border-right: 1px solid var(--border);
      z-index: 30;
      overflow-y: auto;
      transform: translateX(-100%);
      transition: transform 0.25s ease;
    }
    .sidebar.drawer-open {
      transform: translateX(0);
    }
  }
  @media (max-width: 720px) and (prefers-reduced-motion: reduce) {
    .sidebar { transition: none; }
  }
</style>
