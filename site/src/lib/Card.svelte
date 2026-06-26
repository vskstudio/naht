<script>
  import { reveal } from './reveal.js'
  export let span = 1
  export let delay = 0
</script>

<div class="card reveal" style="grid-column: span {span}" use:reveal={{ delay }}>
  <slot />
</div>

<style>
  .card {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-card);
    padding: 18px;
    transition: transform 0.15s ease, border-color 0.2s ease, box-shadow 0.2s ease;
  }
  .card:hover {
    transform: translateY(-3px);
    border-color: var(--border-strong);
    box-shadow: 0 18px 40px -24px rgba(124, 131, 255, 0.5);
  }
  @media (max-width: 560px) { .card { grid-column: span 1 !important; } }
  @media (prefers-reduced-motion: reduce) { .card:hover { transform: none; } }
</style>
