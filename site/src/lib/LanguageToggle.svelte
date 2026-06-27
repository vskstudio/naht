<script>
  import { locale, setLocale, LOCALES } from '../i18n/index.js'
</script>

<div class="toggle" role="group" aria-label="Language">
  {#each LOCALES as code (code)}
    <button
      class="opt"
      class:active={$locale === code}
      aria-pressed={$locale === code}
      onclick={() => setLocale(code)}
    >{code.toUpperCase()}</button>
  {/each}
</div>

<style>
  .toggle {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 9px;
    overflow: hidden;
    font-family: var(--mono);
  }
  .opt {
    background: transparent;
    color: var(--text-dim);
    border: none;
    padding: 6px 11px;
    font-size: 0.74rem;
    font-family: inherit;
    cursor: pointer;
    transition: color 0.15s ease, background 0.15s ease;
  }
  .opt:hover { color: var(--text); }
  .opt.active {
    color: #1a1205;
    background: linear-gradient(120deg, var(--fs), #f0a030);
    font-weight: 700;
  }
</style>
