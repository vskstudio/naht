<script>
  import { t } from '../i18n/index.js'
  import logo from '../assets/logo.png'
  import LanguageToggle from './LanguageToggle.svelte'
  import Icon from './Icon.svelte'
</script>

<nav>
  <div class="wrap nav-inner">
    <a class="brand" href="#/"><img class="brand-mark" src={logo} alt="" width="26" height="26" /> naht</a>
    <div class="nav-links">
      <a class="back" href="#/"><Icon name="arrowLeft" size={15} /> {$t.legal.back}</a>
      <LanguageToggle />
    </div>
  </div>
</nav>

<main>
  <div class="wrap">
    <header class="legal-head">
      <p class="eyebrow">{$t.legal.eyebrow}</p>
      <h1>{$t.legal.title}</h1>
      <p class="lead">{$t.legal.intro}</p>
    </header>

    <dl class="legal-grid">
      {#each $t.legal.rows as r (r.k)}
        <div class="legal-row"><dt>{r.k}</dt><dd>{r.v}</dd></div>
      {/each}
    </dl>

    <p class="foot-license">{$t.footer.license}</p>
  </div>
</main>

<style>
  nav { position: sticky; top: 0; z-index: 50; backdrop-filter: blur(12px); background: rgba(7,8,9,.72); border-bottom: 1px solid var(--border-soft); }
  .nav-inner { display: flex; align-items: center; justify-content: space-between; height: 62px; }
  .brand { display: inline-flex; align-items: center; gap: 9px; font-family: var(--mono); font-weight: 700; font-size: 1.1rem; color: var(--text); }
  .brand-mark { display: block; width: 26px; height: 26px; }
  .nav-links { display: flex; align-items: center; gap: 22px; }
  .back { display: inline-flex; align-items: center; gap: 7px; color: var(--text-dim); font-size: 0.9rem; }
  .back:hover { color: var(--text); }

  main { min-height: calc(100vh - 62px); padding: 64px 0 80px; }
  .legal-head { max-width: 70ch; margin-bottom: 40px; }
  .eyebrow { font-family: var(--mono); font-size: .72rem; letter-spacing: .16em; text-transform: uppercase; color: var(--fs); margin: 0 0 14px; }
  .legal-head h1 { font-size: 2rem; margin: 0 0 16px; color: var(--text); }
  .lead { color: var(--text-dim); font-size: 0.95rem; line-height: 1.6; margin: 0; }

  .legal-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 22px 32px; margin: 0; padding: 28px 0; border-top: 1px solid var(--border-soft); border-bottom: 1px solid var(--border-soft); }
  .legal-row { display: flex; flex-direction: column; gap: 4px; }
  .legal-row dt { font-family: var(--mono); font-size: 0.66rem; letter-spacing: .05em; text-transform: uppercase; color: var(--text-faint); }
  .legal-row dd { margin: 0; font-size: 0.9rem; color: var(--text-dim); line-height: 1.5; }

  .foot-license { color: var(--text-faint); font-size: 0.82rem; margin-top: 28px; }

  @media (max-width: 560px) {
    main { padding: 40px 0 56px; }
    .legal-head h1 { font-size: 1.6rem; }
  }
</style>
