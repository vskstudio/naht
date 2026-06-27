<script>
  import TopBar from './TopBar.svelte'
  import Sidebar from './Sidebar.svelte'
  import MarkdownView from './MarkdownView.svelte'
  export let slug
  let toc = []
</script>

<TopBar />
<div class="shell">
  <Sidebar {slug} />
  <main class="content"><MarkdownView {slug} bind:toc /></main>
  <div class="rail"><!-- TocRight lands here in Task 4 --></div>
</div>

<style>
  .shell {
    display: grid; grid-template-columns: 256px minmax(0, 1fr) 240px;
    max-width: 1380px; margin: 0 auto; align-items: start;
  }
  .content { padding: 40px 48px; min-width: 0; }
  .rail { padding: 40px 16px; }
  @media (max-width: 960px) { .shell { grid-template-columns: 256px minmax(0,1fr); } .rail { display: none; } }
  @media (max-width: 720px) { .shell { grid-template-columns: 1fr; } .content { padding: 28px 20px; } }
</style>
