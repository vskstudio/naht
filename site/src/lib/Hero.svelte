<script>
  import Icon from './Icon.svelte'
  import SeamPanel from './SeamPanel.svelte'
  import { reveal } from './reveal.js'
  import { t } from '../i18n/index.js'
</script>

<header id="top" class="hero">
  <div class="wrap hero-grid">
    <div class="copy">
      <span class="pill reveal" use:reveal><span class="g"></span> {$t.hero.pill}</span>
      <h1 class="reveal" use:reveal={{ delay: 60 }}>{$t.hero.title1}<span class="grad">{$t.hero.titleAccent}</span>{$t.hero.title2}</h1>
      <p class="hero-sub reveal" use:reveal={{ delay: 120 }}>{$t.hero.sub}</p>
      <div class="cta-row reveal" use:reveal={{ delay: 180 }}>
        <a class="btn primary" href="#start"><Icon name="arrow" size={17} /> {$t.hero.ctaPrimary}</a>
        <a class="btn ghost" href="#merge"><Icon name="sync" size={17} /> {$t.hero.ctaSecondary}</a>
      </div>
      <div class="hero-meta reveal" use:reveal={{ delay: 240 }}>
        {#each $t.hero.meta as m (m)}<span>{m}</span>{/each}
      </div>
    </div>
    <div class="hero-visual reveal" use:reveal={{ delay: 160 }}>
      <SeamPanel />
    </div>
  </div>
</header>

<style>
  .hero { padding: 84px 0 104px; }
  .hero-grid { display: grid; grid-template-columns: 1fr 1.15fr; gap: 80px; align-items: center; }
  .pill { display: inline-flex; align-items: center; gap: 10px; font-family: var(--mono); font-size: .74rem; letter-spacing: .05em; color: var(--text-dim); border: 1px solid var(--border); background: #0a0b0e; border-radius: 999px; padding: 6px 13px; margin-bottom: 26px; }
  .pill .g { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 9px var(--ok); }
  h1 { font-size: clamp(2.6rem, 4.4vw, 4.5rem); letter-spacing: -.04em; line-height: 1.02; }
  .hero-sub { color: var(--text-dim); font-size: 1.2rem; max-width: 48ch; margin: 24px 0 34px; }
  .cta-row { display: flex; gap: 14px; flex-wrap: wrap; }
  .hero-meta { display: flex; gap: 22px; margin-top: 30px; font-family: var(--mono); font-size: .74rem; color: var(--text-faint); flex-wrap: wrap; }
  .hero-visual { min-width: 0; }
  @media (max-width: 900px) { .hero-grid { grid-template-columns: 1fr; gap: 48px; } }
</style>
