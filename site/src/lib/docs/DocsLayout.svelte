<script>
  import { onMount, tick } from 'svelte'
  import TopBar from './TopBar.svelte'
  import Sidebar from './Sidebar.svelte'
  import MarkdownView from './MarkdownView.svelte'
  import TocRight from './TocRight.svelte'
  import Search from './Search.svelte'
  import { route } from './router.js'
  import { locale } from '../../i18n/index.js'
  export let slug
  let toc = []
  let searchOpen = false
  let menuOpen = false

  // Close drawer on any slug change (back/forward nav, link click)
  $: slug, (menuOpen = false)

  function prefersReduced() {
    return typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches
  }
  async function scrollToAnchor(anchor) {
    await tick()
    requestAnimationFrame(() => {
      if (anchor) {
        const el = document.getElementById(anchor)
        if (el) { el.scrollIntoView({ behavior: prefersReduced() ? 'auto' : 'smooth', block: 'start' }); return }
      }
      window.scrollTo({ top: 0, behavior: 'auto' })
    })
  }
  $: routeKey = `${$route.slug}#${$route.anchor}`
  $: routeKey, scrollToAnchor($route.anchor)

  onMount(() => {
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); searchOpen = !searchOpen }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })
</script>

<TopBar onSearch={() => (searchOpen = true)} onMenu={() => (menuOpen = !menuOpen)} {menuOpen} />
{#if menuOpen}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <div class="drawer-backdrop" on:click={() => (menuOpen = false)} role="presentation"></div>
{/if}
<div class="shell">
  <Sidebar {slug} locale={$locale} open={menuOpen} onClose={() => (menuOpen = false)} />
  <main class="content">{#key `${slug}-${$locale}`}<MarkdownView {slug} locale={$locale} bind:toc />{/key}</main>
  <div class="rail">{#key `${slug}-${$locale}`}<TocRight {toc} {slug} locale={$locale} />{/key}</div>
</div>
<Search bind:open={searchOpen} locale={$locale} />

<style>
  .shell {
    display: grid; grid-template-columns: 288px minmax(0, 1fr) 264px;
    width: 100%; align-items: start;
  }
  .shell > :global(.sidebar) { border-right: 1px solid var(--border-soft); }
  .content {
    padding: 52px clamp(40px, 6vw, 96px); min-width: 0;
    border-right: 1px solid var(--border-soft);
  }
  .rail { padding: 52px 24px; }
  @media (max-width: 1100px) {
    .shell { grid-template-columns: 264px minmax(0,1fr); }
    .content { border-right: none; }
    .rail { display: none; }
  }
  @media (max-width: 720px) { .shell { grid-template-columns: 1fr; } .content { padding: 28px 20px; } }
  .drawer-backdrop { display: none; }
  @media (max-width: 720px) {
    .drawer-backdrop {
      display: block;
      position: fixed;
      inset: 56px 0 0 0;
      z-index: 29;
      background: rgba(0, 0, 0, 0.55);
    }
  }
</style>
