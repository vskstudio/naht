<script>
  import { reveal } from './reveal.js'

  let {
    num = '',
    label = '',
    title = '',
    tag = '',
    tagCls = '',
    flip = false,
    id = undefined,
    lead,
    children,
  } = $props()
</script>

<section class="row" class:flip {id}>
  <div class="copy">
    {#if num}
      <div class="idx reveal" use:reveal><span class="n">{num}</span> {label} <span class="bar"></span></div>
    {/if}
    {#if tag}<span class="ftag {tagCls} reveal" use:reveal>{tag}</span>{/if}
    <h2 class="section-title reveal" use:reveal>{title}</h2>
    <div class="section-lead reveal" use:reveal>{@render lead?.()}</div>
  </div>
  <div class="visual reveal" use:reveal={{ delay: 120 }}>
    {@render children?.()}
  </div>
</section>

<style>
  .row { display: grid; grid-template-columns: 1fr 1.08fr; gap: 72px; align-items: center; max-width: var(--maxw); margin: 0 auto; padding: 92px 24px; }
  .row.flip .copy { order: 2; } .row.flip .visual { order: 1; }
  .visual { min-width: 0; }
  .idx { display: flex; align-items: center; gap: 12px; font-family: var(--mono); font-size: .72rem; letter-spacing: .16em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 18px; }
  .idx .n { color: var(--fs); } .idx .bar { flex: 1; height: 1px; max-width: 64px; background: var(--border-strong); }
  .ftag { display: inline-flex; align-items: center; gap: 8px; font-family: var(--mono); font-size: .72rem; letter-spacing: .04em; color: var(--fs); background: rgba(245,181,74,.1); border: 1px solid rgba(245,181,74,.22); padding: 4px 11px; border-radius: 999px; margin-bottom: 18px; }
  .ftag.m { color: var(--merge); background: rgba(154,160,255,.1); border-color: rgba(154,160,255,.22); }
  .ftag.s { color: var(--studio); background: rgba(92,200,255,.1); border-color: rgba(92,200,255,.22); }
  .ftag.o { color: var(--ok); background: rgba(74,210,149,.1); border-color: rgba(74,210,149,.22); }
  @media (max-width: 900px) { .row, .row.flip { grid-template-columns: 1fr; gap: 40px; padding: 64px 24px; } .row.flip .copy, .row.flip .visual { order: 0; } }
</style>
