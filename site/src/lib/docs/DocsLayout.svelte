<script>
  import { onMount } from 'svelte'
  import TopBar from './TopBar.svelte'
  import Sidebar from './Sidebar.svelte'
  import MarkdownView from './MarkdownView.svelte'
  import TocRight from './TocRight.svelte'
  import Search from './Search.svelte'
  export let slug
  let toc = []
  let searchOpen = false
  let menuOpen = false

  // Close drawer on any slug change (back/forward nav, link click)
  $: slug, (menuOpen = false)

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
  <Sidebar {slug} open={menuOpen} onClose={() => (menuOpen = false)} />
  <main class="content">{#key slug}<MarkdownView {slug} bind:toc />{/key}</main>
  <div class="rail">{#key slug}<TocRight {toc} />{/key}</div>
</div>
<Search bind:open={searchOpen} />

<style>
  .shell {
    display: grid; grid-template-columns: 256px minmax(0, 1fr) 240px;
    max-width: 1380px; margin: 0 auto; align-items: start;
  }
  .content { padding: 40px 48px; min-width: 0; }
  .rail { padding: 40px 16px; }
  @media (max-width: 960px) { .shell { grid-template-columns: 256px minmax(0,1fr); } .rail { display: none; } }
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
