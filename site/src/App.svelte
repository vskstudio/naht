<script>
  import { route } from './lib/docs/router.js'
  import Landing from './lib/Landing.svelte'
</script>

{#if $route.name === 'docs'}
  <div style="padding:80px;color:var(--text);font-family:var(--mono)">
    docs stub — slug: {$route.slug}
  </div>
{:else}
  <Landing />
{/if}
