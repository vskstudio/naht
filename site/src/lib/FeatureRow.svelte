<script>
  import Icon from './Icon.svelte'
  import { reveal } from './reveal.js'
  export let eyebrow = ''
  export let icon = ''
  export let title = ''
  export let flip = false
  export let id = undefined
</script>

<section class="row" class:flip id={id}>
  <div class="copy">
    {#if eyebrow}
      <div class="eyebrow reveal" use:reveal>
        {#if icon}<Icon name={icon} size={14} />{/if} {eyebrow}
      </div>
    {/if}
    <h2 class="section-title reveal" use:reveal>{title}</h2>
    <div class="section-lead reveal" use:reveal><slot name="lead" /></div>
  </div>
  <div class="visual reveal" use:reveal={{ delay: 120 }}>
    <slot />
  </div>
</section>

<style>
  .row {
    display: grid; grid-template-columns: 1fr 1.05fr; gap: 44px; align-items: center;
    max-width: var(--maxw); margin: 0 auto; padding: 72px 24px;
  }
  .row.flip .copy { order: 2; }
  .row.flip .visual { order: 1; }
  .visual { min-width: 0; }
  @media (max-width: 720px) {
    .row, .row.flip { grid-template-columns: 1fr; gap: 28px; padding: 52px 24px; }
    .row.flip .copy, .row.flip .visual { order: 0; }
  }
</style>
