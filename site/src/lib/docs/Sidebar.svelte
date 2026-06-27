<script>
  import { sections, sectionTitles } from './nav.js'
  import { getMeta } from './registry.js'
  import Icon from '../Icon.svelte'
  export let slug
  export let locale = 'en'
  export let open = false
  export let onClose = () => {}

  $: titles = sectionTitles[locale] ?? sectionTitles.en
</script>

<aside class="sidebar" class:drawer-open={open}>
  <nav>
    {#each sections as section}
      <div class="group">
        <div class="group-title">
          <Icon name={section.icon} size={13} stroke={2.2} />
          <span>{titles[section.id]}</span>
        </div>
        {#each section.items as item}
          <a
            class="link accent-{item.accent}"
            class:active={item.slug === slug}
            href={`#/docs/${item.slug}`}
            on:click={onClose}
          >
            <Icon name={item.icon} size={16} stroke={1.9} />
            <span>{getMeta(locale, item.slug).title ?? item.slug}</span>
          </a>
        {/each}
      </div>
    {/each}
  </nav>
</aside>

<style>
  .sidebar { padding: 30px 14px; }
  nav { position: sticky; top: 76px; }
  .group { margin-bottom: 26px; }
  .group-title {
    display: flex; align-items: center; gap: 8px;
    font-family: var(--mono); font-size: 0.68rem; letter-spacing: 0.1em; text-transform: uppercase;
    color: var(--text-faint); margin-bottom: 10px; padding: 0 10px;
  }
  .group-title :global(svg) { opacity: 0.6; }
  .link {
    display: flex; align-items: center; gap: 10px;
    padding: 7px 10px; border-radius: 8px; color: var(--text-dim); font-size: 0.92rem;
    position: relative; transition: color 0.14s ease, background 0.14s ease;
  }
  .link :global(svg) { flex-shrink: 0; opacity: 0.7; transition: opacity 0.14s ease, color 0.14s ease; }
  .link:hover { color: var(--text); background: var(--bg-soft); }
  .link:hover :global(svg) { opacity: 1; }
  .link.active { color: var(--text); background: var(--bg-soft); font-weight: 550; }
  .link.active::before {
    content: ''; position: absolute; left: -14px; top: 50%; transform: translateY(-50%);
    width: 3px; height: 18px; border-radius: 0 3px 3px 0; background: var(--accent, var(--fs));
  }
  .link.active :global(svg) { opacity: 1; color: var(--accent, var(--fs)); }
  .accent-fs { --accent: var(--fs); }
  .accent-studio { --accent: var(--studio); }
  .accent-merge { --accent: var(--merge); }

  @media (max-width: 720px) {
    .sidebar {
      position: fixed; top: 56px; left: 0; bottom: 0;
      width: min(280px, 85vw); background: var(--bg);
      border-right: 1px solid var(--border); z-index: 30; overflow-y: auto;
      transform: translateX(-100%); transition: transform 0.25s ease;
    }
    .sidebar.drawer-open { transform: translateX(0); }
    nav { position: static; }
  }
  @media (max-width: 720px) and (prefers-reduced-motion: reduce) {
    .sidebar { transition: none; }
  }
</style>
