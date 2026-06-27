<script>
  import { t } from '../i18n/index.js'
</script>

<div class="win">
  <div class="tbar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">{$t.nondestructive.termTitle}</span></div>
  <div class="log">
    {#each $t.nondestructive.logs as line, i (i)}
      <div class="lr">
        <span class="time">{line.time}</span>
        <span class="lv {line.lv}">{line.lv.toUpperCase()}</span>
        <span class="msg">{@html line.msg}{#if i === $t.nondestructive.logs.length - 1}<span class="logcur"></span>{/if}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .win { position: relative; border: 1px solid var(--border); border-radius: 14px; overflow: hidden; background: #0b0d12; font-family: var(--mono); box-shadow: 0 40px 90px -55px rgba(124,131,255,.4), 0 30px 60px -50px #000; }
  .tbar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: #0d0f15; border-bottom: 1px solid var(--border-soft); }
  .dots { display: flex; gap: 6px; } .dots i { width: 11px; height: 11px; border-radius: 50%; background: var(--border-strong); }
  .ttl { flex: 1; text-align: center; font-size: .7rem; color: var(--text-faint); }
  .log { padding: 16px 18px; font-size: .76rem; line-height: 1.9; background: #08090d; }
  .lr { display: grid; grid-template-columns: 70px 58px 1fr; gap: 12px; align-items: baseline; }
  .time { color: #39414e; font-size: .66rem; }
  .lv { font-size: .6rem; padding: 1px 7px; border-radius: 5px; justify-self: start; align-self: center; }
  .lv.info { background: rgba(92,200,255,.12); color: var(--studio); }
  .lv.warn { background: rgba(245,181,74,.14); color: var(--fs); }
  .lv.err { background: rgba(242,97,106,.14); color: var(--danger); }
  .lv.ok { background: rgba(74,210,149,.14); color: var(--ok); }
  .msg { color: var(--text-dim); }
  .msg :global(b) { color: var(--text); font-weight: 500; }
  .msg :global(.lane) { color: var(--merge); }
  .msg :global(.em-ok) { color: var(--ok); }
  .msg :global(.em-warn) { color: var(--fs); }
  .msg :global(.em-err) { color: var(--danger); }
  .logcur { display: inline-block; width: 7px; height: 13px; background: var(--ok); vertical-align: -2px; margin-left: 5px; animation: blink 1s steps(2) infinite; }
  @keyframes blink { 50% { opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .logcur { animation: none; } }
</style>
