<script>
  import Icon from '../Icon.svelte'
  // type: note | tip | important | warning
  let { type = 'note', title = '', children } = $props()
  const ICON = { note: 'info', tip: 'bulb', important: 'info', warning: 'alert' }
  const LABEL = { note: 'Note', tip: 'Tip', important: 'Important', warning: 'Warning' }
</script>

<div class="callout {type}">
  <div class="callout-head"><Icon name={ICON[type]} size={17} /> <span>{title || LABEL[type]}</span></div>
  <div class="callout-body">{@render children?.()}</div>
</div>

<style>
  .callout {
    --c: var(--text-faint);
    border: 1px solid var(--border-strong); border-left: 3px solid var(--c);
    border-radius: var(--radius-sm); background: var(--bg-soft); padding: 16px 18px; margin: 28px 0;
  }
  .callout-head {
    display: flex; align-items: center; gap: 8px; color: var(--c);
    font-family: var(--mono); font-size: 0.74rem; letter-spacing: 0.07em; text-transform: uppercase;
    font-weight: 600; margin-bottom: 10px;
  }
  .callout-body :global(:first-child) { margin-top: 0; }
  .callout-body :global(:last-child) { margin-bottom: 0; }
  .note { --c: var(--merge); }
  .important { --c: var(--studio); }
  .tip { --c: var(--ok); }
  .warning { --c: var(--fs); }
</style>
