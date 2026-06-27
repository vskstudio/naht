<script>
  import { t } from '../i18n/index.js'
</script>

<div class="win">
  <div class="tbar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">{$t.architecture.winTitle}</span></div>
  <div class="ws">
    <div class="wtree">
      <div class="row"><span class="ico dir"></span><span class="nm">naht/</span></div>
      <div class="row l2"><span class="ico file"></span> Cargo.toml <span class="tag d">{$t.architecture.tomlTag}</span></div>
      <div class="row l2"><span class="ico crate-core"></span><span class="nm">naht-core/</span> <span class="tag core">{$t.architecture.badgeCore}</span></div>
      <div class="row l3"><span class="ico file"></span> reconcile.rs</div>
      <div class="row l3"><span class="ico file"></span> merge.rs</div>
      <div class="row l3"><span class="ico file"></span> diff.rs</div>
      <div class="row l2"><span class="ico crate-d"></span><span class="nm">naht/</span> <span class="tag d">{$t.architecture.badgeDaemon}</span></div>
      <div class="row l3"><span class="ico file"></span> serve.rs</div>
      <div class="row l3"><span class="ico file"></span> watch.rs</div>
      <div class="row l3"><span class="ico file"></span> transport.rs</div>
      <div class="row l2"><span class="ico crate-p"></span><span class="nm">plugin/</span> <span class="tag p">{$t.architecture.badgePlugin}</span></div>
      <div class="row l3"><span class="ico file"></span> init.luau</div>
    </div>
    <div>
      <div class="wcode">
        <div class="cl"><span class="g">1</span><span><span class="s-sec">[workspace]</span></span></div>
        <div class="cl"><span class="g">2</span><span><span class="s-key">members</span> = [<span class="s-str">"naht-core"</span>, <span class="s-str">"naht"</span>]</span></div>
        <div class="cl"><span class="g">3</span><span></span></div>
        {#each $t.architecture.tomlComment as c, i (i)}
          <div class="cl"><span class="g">{i + 4}</span><span><span class="s-cm">{c}</span></span></div>
        {/each}
      </div>
      <div class="wfoot">
        <span class="p">$</span><span class="cmd">{$t.architecture.testCmd}</span><br />
        <span class="dim">{$t.architecture.testRunning}</span><br />
        <span class="pass">{$t.architecture.testPass1}</span><br />
        <span class="pass">{$t.architecture.testPass2}</span><br />
        <span class="res">{$t.architecture.testResult}</span> <span class="dim">{$t.architecture.testResultNote}</span>
      </div>
    </div>
  </div>
</div>

<style>
  .win { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; font-family: var(--mono); box-shadow: 0 40px 90px -55px rgba(124,131,255,.4), 0 30px 60px -50px #000; }
  .tbar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .ttl { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .ws { display: grid; grid-template-columns: 1fr 1fr; }
  .wtree { border-right: 1px solid var(--border-soft); padding: 14px 0; font-size: .78rem; }
  .row { display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 16px; color: var(--text-dim); }
  .ico { width: 13px; height: 13px; border-radius: 3px; flex: none; }
  .ico.dir { background: rgba(154,160,255,.2); border: 1px solid rgba(154,160,255,.45); }
  .ico.crate-core { background: rgba(74,210,149,.22); border: 1px solid rgba(74,210,149,.5); }
  .ico.crate-d { background: rgba(154,160,255,.22); border: 1px solid rgba(154,160,255,.5); }
  .ico.crate-p { background: rgba(92,200,255,.22); border: 1px solid rgba(92,200,255,.5); }
  .ico.file { background: #1b1f27; border: 1px solid var(--border-strong); }
  .row.l2 { padding-left: 32px; } .row.l3 { padding-left: 50px; }
  .nm { color: var(--text); }
  .tag { margin-left: auto; font-size: .58rem; padding: 2px 8px; border-radius: 999px; }
  .tag.core { background: rgba(74,210,149,.12); color: var(--ok); }
  .tag.d { background: rgba(154,160,255,.12); color: var(--merge); }
  .tag.p { background: rgba(92,200,255,.12); color: var(--studio); }
  .wcode { padding: 14px 0; font-size: .74rem; line-height: 1.05; background: #0a0c10; }
  .wcode .cl { display: grid; grid-template-columns: 26px 1fr; align-items: center; height: 24px; padding-right: 14px; }
  .wcode .cl .g { color: #39414e; text-align: right; padding-right: 10px; font-size: .6rem; }
  .s-sec { color: var(--studio); } .s-key { color: var(--fs); } .s-str { color: #7fd6a6; } .s-cm { color: #5b6472; }
  .wfoot { border-top: 1px solid var(--border-soft); background: #08090d; padding: 12px 16px; font-size: .72rem; line-height: 1.75; }
  .wfoot .p { color: var(--fs); margin-right: 7px; } .wfoot .cmd { color: var(--text); }
  .wfoot .pass { color: var(--ok); } .wfoot .dim { color: var(--text-faint); } .wfoot .res { color: var(--ok); }
  @media (max-width: 900px) { .ws { grid-template-columns: 1fr; } .wtree { border-right: none; border-bottom: 1px solid var(--border-soft); } }
</style>
