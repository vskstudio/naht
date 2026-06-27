<script>
  import { t } from '../i18n/index.js'
</script>

<div class="seamvis">
  <div class="lace">
    <div class="pane d">
      <div class="phd"><span class="dotwin"><i></i><i></i><i></i></span> {$t.hero.seam.fsHeader} <span class="tag">PlayerData.lua</span></div>
      <div class="code">
        <div class="cl"><span class="gut">1</span><span><span class="kw">local</span> <span class="id">m</span> <span class="pn">=</span> <span class="pn">{'{}'}</span></span></div>
        <div class="cl hot"><span class="gut">2</span><span><span class="id">m</span><span class="pn">.</span><span class="fn2">speed</span> <span class="pn">=</span> <span class="num">16</span></span></div>
        <div class="cl"><span class="gut">3</span><span><span class="id">m</span><span class="pn">.</span><span class="fn2">jump</span> <span class="pn">=</span> <span class="num">50</span></span></div>
        <div class="cl"><span class="gut">4</span><span><span class="id">m</span><span class="pn">.</span><span class="fn2">dash</span> <span class="pn">=</span> <span class="bool">true</span></span></div>
        <div class="cl"><span class="gut">5</span><span></span></div>
        <div class="cl"><span class="gut">6</span><span><span class="kw">return</span> <span class="id">m</span></span></div>
      </div>
    </div>

    <div class="gap">
      <svg viewBox="0 0 168 272">
        <defs><linearGradient id="lr" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#f5b54a" /><stop offset=".5" stop-color="#9aa0ff" /><stop offset="1" stop-color="#5cc8ff" />
        </linearGradient></defs>
        <path class="t2" stroke="url(#lr)" d="M10,56 C84,56 84,136 158,136 M10,136 C84,136 84,216 158,216 M10,216 C84,216 84,56 158,56" />
        <path class="t2dash" stroke="url(#lr)" d="M10,56 C84,56 84,136 158,136 M10,136 C84,136 84,216 158,216 M10,216 C84,216 84,56 158,56" />
        <circle class="hole d" cx="10" cy="56" r="3.6" /><circle class="hole d" cx="10" cy="136" r="3.6" /><circle class="hole d" cx="10" cy="216" r="3.6" />
        <circle class="hole s" cx="158" cy="56" r="3.6" /><circle class="hole s" cx="158" cy="136" r="3.6" /><circle class="hole s" cx="158" cy="216" r="3.6" />
      </svg>
      <div class="chip toS">.lua</div>
      <div class="chip toD">.lua</div>
    </div>

    <div class="pane s">
      <div class="phd">{$t.hero.seam.studioHeader} <span class="tag">ServerStorage</span></div>
      <div class="tree">
        <div class="tr"><span class="tw">▾</span><span class="ico svc"></span> ServerStorage</div>
        <div class="tr l2"><span class="tw">▾</span><span class="ico fold"></span> Naht</div>
        <div class="tr l3 hot"><span class="tw"></span><span class="ico mod"></span> PlayerData</div>
        <div class="tr l3"><span class="tw"></span><span class="ico mod"></span> Shop</div>
        <div class="tr l3"><span class="tw"></span><span class="ico mod"></span> Combat</div>
        <div class="tr l3"><span class="tw"></span><span class="ico mod"></span> Util</div>
      </div>
    </div>
  </div>
  <div class="status"><span class="g"></span> {$t.hero.seam.status}</div>
</div>

<style>
  .seamvis { position: relative; }
  .seamvis::before { content: ''; position: absolute; inset: -8% -4%; z-index: -1; background: radial-gradient(60% 60% at 50% 50%, rgba(154,160,255,.10), transparent 70%); filter: blur(8px); }
  .lace { display: grid; grid-template-columns: 1fr 168px 1fr; align-items: center; }
  .pane { border: 1px solid var(--border); border-radius: 16px; background: linear-gradient(180deg,#0d0f15,#0a0b0e); overflow: hidden; box-shadow: 0 40px 80px -50px rgba(0,0,0,.9); }
  .pane.d { box-shadow: 0 40px 80px -48px rgba(245,181,74,.25), 0 30px 60px -50px #000; }
  .pane.s { box-shadow: 0 40px 80px -48px rgba(92,200,255,.25), 0 30px 60px -50px #000; }
  .phd { display: flex; align-items: center; gap: 8px; padding: 13px 15px; border-bottom: 1px solid var(--border-soft); font-family: var(--mono); font-size: .64rem; letter-spacing: .05em; text-transform: uppercase; }
  .pane.d .phd { color: var(--fs); } .pane.s .phd { color: var(--studio); }
  .phd .tag { margin-left: auto; color: var(--text-faint); text-transform: none; }
  .dotwin { display: flex; gap: 5px; } .dotwin i { width: 9px; height: 9px; border-radius: 50%; background: var(--border-strong); }
  .code { padding: 14px 0; font-family: var(--mono); font-size: .76rem; line-height: 1; }
  .cl { display: grid; grid-template-columns: 30px 1fr; align-items: center; height: 34px; padding-right: 16px; }
  .cl .gut { color: #3a3f4a; text-align: right; padding-right: 12px; font-size: .66rem; }
  .cl.hot { background: linear-gradient(90deg, rgba(245,181,74,.16), transparent); }
  .kw { color: #d98ad9; } .id { color: var(--text); } .num { color: #7fd6a6; } .fn2 { color: var(--fs); } .pn { color: var(--text-dim); } .bool { color: #7fd6a6; }
  .tree { padding: 14px 0; font-family: var(--mono); font-size: .76rem; }
  .tr { display: flex; align-items: center; gap: 8px; height: 34px; padding: 0 16px; color: var(--text-dim); }
  .tr .tw { color: var(--text-faint); width: 10px; } .tr .ico { width: 13px; height: 13px; border-radius: 3px; }
  .ico.svc { background: rgba(92,200,255,.25); border: 1px solid rgba(92,200,255,.5); }
  .ico.fold { background: rgba(154,160,255,.22); border: 1px solid rgba(154,160,255,.5); }
  .ico.mod { background: rgba(92,200,255,.35); border: 1px solid var(--studio); }
  .tr.l2 { padding-left: 32px; } .tr.l3 { padding-left: 50px; }
  .tr.hot { background: linear-gradient(90deg, transparent, rgba(92,200,255,.18)); color: var(--text); }
  .gap { position: relative; width: 168px; height: 272px; justify-self: center; }
  .gap svg { position: absolute; inset: 0; width: 168px; height: 272px; overflow: visible; }
  .hole { fill: #070809; stroke-width: 1.7; } .hole.d { stroke: var(--fs); } .hole.s { stroke: var(--studio); }
  .t2 { stroke-width: 2; fill: none; stroke-linecap: round; opacity: .4; filter: drop-shadow(0 0 4px rgba(154,160,255,.5)); }
  .t2dash { stroke-width: 2.2; fill: none; stroke-dasharray: 5 300; stroke-linecap: round; animation: bead 3s linear infinite; }
  @keyframes bead { to { stroke-dashoffset: -305; } }
  .chip { position: absolute; top: 0; left: 0; width: 42px; height: 25px; border-radius: 8px; display: grid; place-items: center; font-family: var(--mono); font-size: .56rem; font-weight: 700; color: #0c0d10; offset-rotate: 0deg; offset-anchor: 50% 50%; }
  .chip.toS { background: linear-gradient(120deg, var(--fs), var(--merge)); box-shadow: 0 0 22px rgba(245,181,74,.8); offset-path: path('M10 56 C84 56 84 136 158 136'); animation: ride 3.6s ease-in-out infinite; }
  .chip.toD { background: linear-gradient(120deg, var(--studio), var(--merge)); box-shadow: 0 0 22px rgba(92,200,255,.8); offset-path: path('M158 216 C84 216 84 136 10 136'); animation: ride 3.6s ease-in-out infinite; animation-delay: 1.8s; }
  @keyframes ride { 0% { offset-distance: 0%; opacity: 0; } 10% { opacity: 1; } 90% { opacity: 1; } 100% { offset-distance: 100%; opacity: 0; } }
  .status { display: flex; align-items: center; gap: 10px; justify-content: center; margin-top: 22px; font-family: var(--mono); font-size: .74rem; color: var(--ok); }
  .status .g { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 10px var(--ok); }
  @media (prefers-reduced-motion: reduce) {
    .t2dash, .chip.toS, .chip.toD { animation: none; }
    .chip.toS { offset-distance: 50%; } .chip.toD { offset-distance: 50%; }
  }
</style>
