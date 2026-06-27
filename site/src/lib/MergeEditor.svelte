<script>
  import { t } from '../i18n/index.js'
</script>

<div class="vsc">
  <div class="titlebar"><span class="dots"><i></i><i></i><i></i></span><span class="title">naht — merge editor</span></div>
  <div class="tabrow"><span class="tab"><span class="fi"></span> {$t.merge.tab}</span></div>

  <div class="inputs">
    <div class="vpane disk">
      <div class="vphd"><span class="d"></span> {$t.merge.diskHeader} <span class="sub">{$t.merge.diskSub}</span> <span class="accept">{$t.merge.accept}</span></div>
      <div class="ved">
        <div class="vln"><span class="g">1</span><span><span class="vkw">local</span> <span class="vid">m</span> <span class="vpn">=</span> <span class="vpn">{'{}'}</span></span></div>
        <div class="vln cur"><span class="g">2</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">speed</span> <span class="vpn">=</span> <span class="vnum">24</span></span></div>
        <div class="vln"><span class="g">3</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">jump</span> <span class="vpn">=</span> <span class="vnum">50</span></span></div>
      </div>
    </div>
    <div class="vpane studio">
      <div class="vphd"><span class="d"></span> {$t.merge.studioHeader} <span class="sub">{$t.merge.studioSub}</span> <span class="accept">{$t.merge.accept}</span></div>
      <div class="ved">
        <div class="vln"><span class="g">1</span><span><span class="vkw">local</span> <span class="vid">m</span> <span class="vpn">=</span> <span class="vpn">{'{}'}</span></span></div>
        <div class="vln inc"><span class="g">2</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">speed</span> <span class="vpn">=</span> <span class="vnum">32</span></span></div>
        <div class="vln inc"><span class="g">3</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">jump</span> <span class="vpn">=</span> <span class="vnum">65</span></span></div>
      </div>
    </div>
  </div>

  <div class="result">
    <div class="rhd"><span class="d"></span> {$t.merge.resultHeader} <span class="base"><span class="cyl"></span> {$t.merge.baseTag}</span></div>
    <div class="ved">
      <div class="vln"><span class="g">1</span><span><span class="vkw">local</span> <span class="vid">m</span> <span class="vpn">=</span> <span class="vpn">{'{}'}</span></span></div>
      <div class="cf">
        <div class="cfbar">⟂ {$t.merge.conflictBar}
          <span class="actions"><span class="a cur">{$t.merge.actionDisk}</span><span class="a inc">{$t.merge.actionStudio}</span><span class="a">{$t.merge.actionBoth}</span></span>
        </div>
        <div class="cfmark h-cur"><span class="g">◂</span><span>&lt;&lt;&lt;&lt;&lt;&lt;&lt; disk (current)</span></div>
        <div class="cfmark h-cur"><span class="g">2</span><span>m.speed = 24</span></div>
        <div class="cfmark sep"><span class="g"></span><span>=======</span></div>
        <div class="cfmark h-inc"><span class="g">2</span><span>m.speed = 32</span></div>
        <div class="cfmark h-inc"><span class="g">▸</span><span>&gt;&gt;&gt;&gt;&gt;&gt;&gt; studio (incoming)</span></div>
      </div>
      <div class="vln ok"><span class="g gut-ok">3</span><span><span class="vid">m</span><span class="vpn">.</span><span class="vfn">jump</span> <span class="vpn">=</span> <span class="vnum">65</span>  <span class="vcm">{$t.merge.autoMerged}</span></span></div>
      <div class="vln"><span class="g">4</span><span><span class="vkw">return</span> <span class="vid">m</span></span></div>
    </div>
    <div class="vfoot"><span class="gd"></span> {$t.merge.footerClean} <span class="sep">·</span> <span class="frozen">{$t.merge.footerConflict}</span></div>
  </div>
</div>

<style>
  .vsc { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; box-shadow: 0 40px 90px -55px rgba(124,131,255,.45), 0 30px 60px -50px #000; font-family: var(--mono); }
  .titlebar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .title { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .tabrow { display: flex; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .tab { display: inline-flex; align-items: center; gap: 7px; padding: 6px 13px; background: #0b0d12; border: 1px solid var(--border-soft); border-bottom: none; border-radius: 7px 7px 0 0; font-size: .72rem; color: var(--text); margin: 6px 0 -1px 14px; }
  .tab .fi { width: 11px; height: 11px; border-radius: 3px; background: var(--grad); }
  .inputs { display: grid; grid-template-columns: 1fr 1fr; border-bottom: 1px solid var(--border-soft); }
  .vpane { border-right: 1px solid var(--border-soft); } .vpane:last-child { border-right: none; }
  .vphd { display: flex; align-items: center; gap: 8px; padding: 9px 14px; font-size: .66rem; letter-spacing: .04em; background: #0a0c10; border-bottom: 1px solid var(--border-soft); }
  .vphd .d { width: 8px; height: 8px; border-radius: 50%; }
  .vpane.disk .vphd .d { background: var(--fs); } .vpane.disk .vphd { color: var(--fs); }
  .vpane.studio .vphd .d { background: var(--studio); } .vpane.studio .vphd { color: var(--studio); }
  .vphd .sub { margin-left: auto; color: var(--text-faint); text-transform: none; letter-spacing: 0; }
  .accept { font-size: .6rem; color: var(--text-faint); border: 1px solid var(--border-soft); border-radius: 5px; padding: 2px 7px; }
  .ved { font-size: .74rem; line-height: 1; padding: 6px 0; }
  .vln { display: grid; grid-template-columns: 34px 1fr; align-items: center; height: 26px; }
  .vln .g { color: #39414e; text-align: right; padding-right: 11px; font-size: .64rem; }
  .vln.cur { background: linear-gradient(90deg, rgba(245,181,74,.14), transparent); box-shadow: inset 2px 0 0 var(--fs); }
  .vln.inc { background: linear-gradient(90deg, rgba(92,200,255,.14), transparent); box-shadow: inset 2px 0 0 var(--studio); }
  .vln.ok { background: linear-gradient(90deg, rgba(74,210,149,.10), transparent); box-shadow: inset 2px 0 0 var(--ok); }
  .vkw { color: #d98ad9; } .vid { color: var(--text); } .vnum { color: #7fd6a6; } .vfn { color: var(--fs); } .vpn { color: var(--text-dim); } .vcm { color: #5b6472; }
  .gut-ok { color: var(--ok) !important; }
  .rhd { display: flex; align-items: center; gap: 9px; padding: 9px 14px; font-size: .66rem; color: var(--merge); background: rgba(154,160,255,.06); border-bottom: 1px solid var(--border-soft); }
  .rhd .d { width: 8px; height: 8px; border-radius: 50%; background: var(--merge); }
  .rhd .base { margin-left: auto; color: var(--text-faint); font-size: .62rem; display: flex; align-items: center; gap: 6px; }
  .rhd .cyl { width: 13px; height: 16px; border-radius: 50%/22%; border: 1px solid rgba(154,160,255,.5); background: linear-gradient(180deg, rgba(154,160,255,.2), transparent); }
  .cf { border-top: 1px solid var(--border-soft); border-bottom: 1px solid var(--border-soft); }
  .cfbar { display: flex; align-items: center; gap: 10px; padding: 7px 14px; font-size: .62rem; background: rgba(242,97,106,.07); color: var(--danger); }
  .cfbar .actions { margin-left: auto; display: flex; gap: 8px; }
  .cfbar .a { padding: 2px 8px; border-radius: 5px; border: 1px solid var(--border-soft); color: var(--text-dim); }
  .cfbar .a.cur { color: var(--fs); border-color: rgba(245,181,74,.4); }
  .cfbar .a.inc { color: var(--studio); border-color: rgba(92,200,255,.4); }
  .cfmark { display: grid; grid-template-columns: 34px 1fr; align-items: center; height: 24px; font-size: .7rem; }
  .cfmark .g { color: #39414e; text-align: right; padding-right: 11px; font-size: .62rem; }
  .cfmark.h-cur { background: rgba(245,181,74,.10); color: var(--fs); }
  .cfmark.h-inc { background: rgba(92,200,255,.10); color: var(--studio); }
  .cfmark.sep { color: var(--text-faint); }
  .vfoot { display: flex; align-items: center; gap: 9px; padding: 9px 14px; font-size: .64rem; color: var(--ok); background: #0a0c10; border-top: 1px solid var(--border-soft); }
  .vfoot .gd { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 8px var(--ok); }
  .vfoot .sep { color: var(--text-faint); } .vfoot .frozen { color: var(--danger); }
</style>
