<script>
  import { t } from '../i18n/index.js'
</script>

<div class="mx">
  <table>
    <colgroup><col class="cn" /><col /><col /><col /></colgroup>
    <thead>
      <tr><th></th><th class="naht">{$t.matrix.head.naht}</th><th>{$t.matrix.head.rojo}</th><th>{$t.matrix.head.argon}</th></tr>
    </thead>
    <tbody>
      {#each $t.matrix.rows as r (r.label)}
        <tr>
          <th>{r.label}</th>
          <td class="naht"><span class="yes">{r.naht}</span></td>
          <td class={r.rojoCls}>{r.rojo}</td>
          <td class={r.argonCls}>{r.argon}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
<p class="mx-note">{$t.matrix.note}</p>

<style>
  .mx { border: 1px solid var(--border); border-radius: 16px; overflow: hidden; background: var(--bg-card); }
  table { border-collapse: collapse; width: 100%; font-size: .94rem; }
  th, td { padding: 15px 18px; text-align: center; border-top: 1px solid var(--border-soft); }
  thead th { background: #0a0b0e; font-weight: 700; border-top: none; position: relative; }
  tbody th { text-align: left; color: #cdd2db; font-weight: 500; }
  col.cn { width: 34%; }
  .naht { background: rgba(245,181,74,.055); }
  thead .naht { color: var(--fs); }
  thead .naht::after { content: ''; position: absolute; left: 0; right: 0; top: 0; height: 2px; background: var(--grad); }
  .yes { color: var(--ok); font-weight: 700; } .wn { color: var(--fs); font-weight: 700; } .no { color: var(--danger); font-weight: 700; opacity: .85; }
  .naht .yes { font-family: var(--mono); font-size: .82rem; }
  .mx-note { margin-top: 13px; font-family: var(--mono); font-size: .72rem; color: var(--text-faint); }
</style>
