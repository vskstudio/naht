<script>
  import { t } from '../i18n/index.js'

  const rows = [
    { path: 'src/PlayerData.lua', hash: 'a3f1c0…', rev: '#48', ts: '12:04:31', changed: true },
    { path: 'src/Shop.lua', hash: '7be902…', rev: '#46', ts: '12:01:08', changed: false },
    { path: 'src/Combat/init.lua', hash: 'd51aa7…', rev: '#46', ts: '12:01:08', changed: false },
    { path: 'src/Util/math.lua', hash: '0c8e34…', rev: '#39', ts: '11:52:55', changed: false },
  ]
</script>

<div class="win">
  <div class="tbar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">{$t.base.inspectorTitle}</span></div>
  <div class="dbtb"><span class="cyl"></span> base <span class="path">.naht/state.db</span> <span class="q">{$t.base.query}</span></div>
  <table class="tbl">
    <thead><tr><th>{$t.base.cols.path}</th><th>{$t.base.cols.hash}</th><th>{$t.base.cols.rev}</th><th>{$t.base.cols.ts}</th></tr></thead>
    <tbody>
      {#each rows as r (r.path)}
        <tr class:changed={r.changed}>
          <td class="pk">{r.path}</td><td class="hash">{r.hash}</td><td class="rev">{r.rev}</td><td class="ts">{r.ts}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  <div class="dbfoot"><span class="gd"></span> {$t.base.footer}</div>
</div>

<style>
  .win { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; font-family: var(--mono); box-shadow: 0 40px 90px -55px rgba(124,131,255,.4), 0 30px 60px -50px #000; }
  .tbar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .ttl { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .dbtb { display: flex; align-items: center; gap: 10px; padding: 9px 14px; font-size: .66rem; color: var(--merge); background: rgba(154,160,255,.06); border-bottom: 1px solid var(--border-soft); }
  .dbtb .cyl { width: 13px; height: 16px; border-radius: 50%/22%; border: 1px solid rgba(154,160,255,.5); background: linear-gradient(180deg, rgba(154,160,255,.2), transparent); }
  .dbtb .path { color: var(--text-faint); } .dbtb .q { margin-left: auto; color: var(--text-faint); font-size: .62rem; }
  .tbl { width: 100%; border-collapse: collapse; font-size: .74rem; }
  .tbl th { text-align: left; padding: 9px 14px; color: var(--text-faint); font-weight: 500; font-size: .62rem; letter-spacing: .04em; background: #0a0c10; border-bottom: 1px solid var(--border-soft); }
  .tbl td { padding: 9px 14px; border-bottom: 1px solid var(--border-soft); color: var(--text-dim); }
  .tbl tr:last-child td { border-bottom: none; }
  .tbl .pk { color: var(--text); } .tbl .hash { color: var(--studio); } .tbl .rev { color: var(--fs); } .tbl .ts { color: var(--text-faint); }
  .tbl tr.changed td { background: rgba(245,181,74,.05); }
  .dbfoot { display: flex; align-items: center; gap: 9px; padding: 10px 14px; font-size: .64rem; color: var(--ok); background: #0a0c10; border-top: 1px solid var(--border-soft); }
  .dbfoot .gd { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 8px var(--ok); }
</style>
