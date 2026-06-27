<script>
  import { route } from './lib/docs/router.js'
  import Landing from './lib/Landing.svelte'
  import DocsLayout from './lib/docs/DocsLayout.svelte'
</script>

{#if $route.name === 'docs'}
  <DocsLayout slug={$route.slug} />
{:else}
  <Landing />
{/if}
