<script>
  import { route } from './lib/docs/router.js'
  import Landing from './lib/Landing.svelte'
  import MarkdownView from './lib/docs/MarkdownView.svelte'
</script>

{#if $route.name === 'docs'}
  <div style="max-width:900px;margin:0 auto;padding:60px 24px">
    <MarkdownView slug={$route.slug} />
  </div>
{:else}
  <Landing />
{/if}
