<script>
  import { t } from '../i18n/index.js'
</script>

<div class="win">
  <div class="tbar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">{$t.try.termTitle}</span></div>
  <div class="gterm">
    <div class="l"><span class="gp">$</span> <span class="gcmd">naht init demo</span></div>
    <div class="l"><span class="gout">{$t.try.term.scaffold}</span> <span class="gcm">{$t.try.term.scaffoldNote}</span></div>
    <div class="blank"></div>
    <div class="l"><span class="gp">$</span> <span class="gcmd">naht serve</span></div>
    <div class="l"><span class="gtag daemon">daemon</span><span class="gout">{$t.try.term.watching}</span></div>
    <div class="l"><span class="gtag plugin">plugin</span><span class="gout">{$t.try.term.connecting}</span></div>
    <div class="l"><span class="gtag plugin">plugin</span><span class="gok">{$t.try.term.connected}</span> <span class="gout">{$t.try.term.connectedNote}</span></div>
    <div class="blank"></div>
    <div class="l"><span class="gout gcm">{$t.try.term.editDisk}</span></div>
    <div class="l"><span class="gtag daemon">daemon</span><span class="gout">{$t.try.term.appliedFs} <b>PlayerData</b> <span class="gok">{$t.try.term.applied}</span></span></div>
    <div class="l"><span class="gout gcm">{$t.try.term.editStudio}</span></div>
    <div class="l"><span class="gtag daemon">daemon</span><span class="gout">{$t.try.term.appliedStudioPre} <b>Shop</b> <span class="gmg">{$t.try.term.mergeLabel}</span> <span class="gok">{$t.try.term.mergeResult}</span></span></div>
    <div class="blank"></div>
    <div class="l"><span class="gok">●</span> <span class="gout">{$t.try.term.done}<span class="logcur"></span></span></div>
  </div>
</div>

<style>
  .win { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; font-family: var(--mono); box-shadow: 0 40px 90px -55px rgba(124,131,255,.4), 0 30px 60px -50px #000; }
  .tbar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .ttl { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .gterm { padding: 18px 20px; font-size: .82rem; line-height: 1.85; background: #08090d; }
  .l { white-space: pre-wrap; }
  .gp { color: var(--fs); } .gcmd { color: var(--text); } .gout { color: var(--text-dim); } .gout b { color: var(--text); font-weight: 500; } .gcm { color: #5b6472; }
  .gok { color: var(--ok); } .gmg { color: var(--merge); }
  .gtag { display: inline-block; font-size: .64rem; padding: 1px 7px; border-radius: 5px; margin-right: 8px; }
  .gtag.plugin { background: rgba(92,200,255,.14); color: var(--studio); }
  .gtag.daemon { background: rgba(154,160,255,.14); color: var(--merge); }
  .blank { height: 8px; }
  .logcur { display: inline-block; width: 8px; height: 14px; background: var(--ok); vertical-align: -2px; margin-left: 4px; animation: blink 1s steps(2) infinite; }
  @keyframes blink { 50% { opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .logcur { animation: none; } }
</style>
