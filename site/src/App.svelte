<script>
  import { route } from './lib/docs/router.js'
  import Landing from './lib/Landing.svelte'
  import DocsLayout from './lib/docs/DocsLayout.svelte'
  import Legal from './lib/Legal.svelte'
</script>

{#if $route.name === 'docs'}
  <DocsLayout slug={$route.slug} />
{:else if $route.name === 'legal'}
  <Legal />
{:else}
  <Landing />
{/if}
