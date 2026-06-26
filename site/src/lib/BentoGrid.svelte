<script>
  export let cols = 3
</script>

<div class="bento" style="--cols:{cols}">
  <slot />
</div>

<style>
  .bento {
    display: grid;
    grid-template-columns: repeat(var(--cols), 1fr);
    gap: 16px;
  }
  @media (max-width: 860px) { .bento { grid-template-columns: repeat(2, 1fr); } }
  @media (max-width: 560px) { .bento { grid-template-columns: 1fr; } }
</style>
