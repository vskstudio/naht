<script>
  import Icon from './Icon.svelte'
  import { reveal } from './reveal.js'
  import { t } from '../i18n/index.js'
  import logo from '../assets/logo.png'
  import Hero from './Hero.svelte'
  import LanguageToggle from './LanguageToggle.svelte'
  import FeatureRow from './FeatureRow.svelte'
  import ComparisonMatrix from './ComparisonMatrix.svelte'
  import MergeEditor from './MergeEditor.svelte'
  import StateInspector from './StateInspector.svelte'
  import LogStream from './LogStream.svelte'
  import WorkspaceTree from './WorkspaceTree.svelte'
  import GuidedTerminal from './GuidedTerminal.svelte'

  const REPO = 'https://github.com/vskstudio/naht'
</script>

<nav>
  <div class="wrap nav-inner">
    <a class="brand" href="#top"><img class="brand-mark" src={logo} alt="" width="26" height="26" /> naht</a>
    <div class="nav-links">
      <a href="#comparison">{$t.nav.comparison}</a>
      <a href="#merge">{$t.nav.merge}</a>
      <a href="#architecture">{$t.nav.architecture}</a>
      <a href="#start">{$t.nav.quickstart}</a>
      <a class="keep" href="#/docs">Docs</a>
      <a class="nav-cta" href={REPO} target="_blank" rel="noreferrer"><Icon name="git" size={16} /> {$t.nav.github}</a>
    </div>
  </div>
</nav>

<Hero />

<main>
  <!-- 01 Comparison -->
  <section id="comparison">
    <div class="wrap">
      <div class="idx reveal" use:reveal><span class="n">{$t.matrix.num}</span> {$t.matrix.label} <span class="bar"></span></div>
      <h2 class="section-title reveal" use:reveal>{$t.matrix.title}</h2>
      <p class="section-lead reveal" use:reveal>{$t.matrix.lead}</p>
      <div class="reveal" use:reveal={{ delay: 80 }}><ComparisonMatrix /></div>
    </div>
  </section>

  <!-- 02 Merge -->
  <section id="merge" class="alt">
    <div class="wrap">
      <div class="idx reveal" use:reveal><span class="n">{$t.merge.num}</span> Merge 3-way <span class="bar"></span></div>
      <span class="ftag m reveal" use:reveal>{$t.merge.tag}</span>
      <h2 class="section-title reveal narrow" use:reveal>{$t.merge.title}</h2>
      <p class="section-lead reveal" use:reveal>{$t.merge.lead} <span class="muted">{$t.merge.leadVs}</span></p>
      <div class="reveal" use:reveal={{ delay: 80 }}><MergeEditor /></div>
    </div>
  </section>

  <!-- 03 Base -->
  <FeatureRow id="base-persisted" num={$t.base.num} label={$t.nav.base} title={$t.base.title} tag={$t.base.tag} flip>
    {#snippet lead()}{$t.base.lead} <span class="muted">{$t.base.leadVs}</span>{/snippet}
    <StateInspector />
  </FeatureRow>

  <!-- 04 Non-destructive -->
  <FeatureRow num={$t.nondestructive.num} label={$t.nav.nondestructive} title={$t.nondestructive.title} tag={$t.nondestructive.tag} tagCls="o">
    {#snippet lead()}{$t.nondestructive.lead} <span class="muted">{$t.nondestructive.leadVs}</span>{/snippet}
    <LogStream />
  </FeatureRow>

  <!-- 05 Architecture -->
  <FeatureRow id="architecture" num={$t.architecture.num} label={$t.nav.architecture} title={$t.architecture.title} tag={$t.architecture.tag} tagCls="s" flip>
    {#snippet lead()}{$t.architecture.lead} <span class="muted">{$t.architecture.leadVs}</span>{/snippet}
    <WorkspaceTree />
  </FeatureRow>

  <!-- 06 Try -->
  <section id="start" class="alt">
    <div class="wrap">
      <div class="idx reveal" use:reveal><span class="n">{$t.try.num}</span> {$t.try.label} <span class="bar"></span></div>
      <h2 class="section-title reveal" use:reveal>{$t.try.title}</h2>
      <p class="section-lead reveal" use:reveal>{$t.try.lead}</p>
      <div class="reveal" use:reveal={{ delay: 80 }}><GuidedTerminal /></div>

      <div class="limits-note reveal" use:reveal>
        <h4>{$t.try.limitsTitle}</h4>
        {#each $t.try.limits as l (l.label)}
          <span class="lpill">{l.label} <span class="s {l.cls}">{l.status}</span></span>
        {/each}
      </div>

      <div class="start-cta reveal" use:reveal>
        <a class="btn primary" href={REPO + '/blob/main/docs/quickstart.md'} target="_blank" rel="noreferrer"><Icon name="arrow" size={17} /> {$t.try.ctaPrimary}</a>
        <a class="btn ghost" href={REPO + '/blob/main/docs/architecture.md'} target="_blank" rel="noreferrer"><Icon name="layers" size={17} /> {$t.try.ctaSecondary}</a>
      </div>
    </div>
  </section>
</main>

<footer>
  <div class="wrap">
    <div class="foot-inner">
      <div class="foot-brand">
        <img class="brand-mark" src={logo} alt="" width="30" height="30" />
        <div><strong>naht</strong><span>{$t.footer.tagline}</span></div>
      </div>
      <div class="foot-links">
        <a href={REPO} target="_blank" rel="noreferrer">{$t.footer.links.github}</a>
        <a href="#/docs">Docs</a>
        <a href={REPO + '/blob/main/docs/quickstart.md'} target="_blank" rel="noreferrer">{$t.footer.links.quickstart}</a>
        <a href={REPO + '/blob/main/docs/architecture.md'} target="_blank" rel="noreferrer">{$t.footer.links.architecture}</a>
      </div>
      <LanguageToggle />
    </div>

    <div class="foot-bottom">
      <span class="foot-license">{$t.footer.license}</span>
      <a class="foot-legal-link" href="#/legal">{$t.legal.title}</a>
    </div>
  </div>
</footer>

<style>
  nav { position: sticky; top: 0; z-index: 50; backdrop-filter: blur(12px); background: rgba(7,8,9,.72); border-bottom: 1px solid var(--border-soft); }
  .nav-inner { display: flex; align-items: center; justify-content: space-between; height: 62px; }
  .brand { display: inline-flex; align-items: center; gap: 9px; font-family: var(--mono); font-weight: 700; font-size: 1.1rem; color: var(--text); }
  .brand-mark { display: block; width: 26px; height: 26px; }
  .nav-links { display: flex; align-items: center; gap: 22px; }
  .nav-links a { color: var(--text-dim); font-size: 0.9rem; }
  .nav-links a:hover { color: var(--text); }
  .nav-cta { display: inline-flex; align-items: center; gap: 7px; padding: 7px 14px; border: 1px solid var(--border); border-radius: 9px; color: var(--text) !important; }
  .nav-cta:hover { border-color: var(--fs); }
  @media (max-width: 820px) { .nav-links a:not(.nav-cta):not(.keep) { display: none; } .nav-links { gap: 14px; } }
  @media (max-width: 420px) { .nav-cta { font-size: 0.84rem; padding: 6px 10px; } }

  section.alt { background: var(--bg-soft); border-top: 1px solid var(--border-soft); border-bottom: 1px solid var(--border-soft); }
  .idx { display: flex; align-items: center; gap: 12px; font-family: var(--mono); font-size: .72rem; letter-spacing: .16em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 18px; }
  .idx .n { color: var(--fs); } .idx .bar { flex: 1; height: 1px; max-width: 64px; background: var(--border-strong); }
  .ftag { display: inline-flex; align-items: center; gap: 8px; font-family: var(--mono); font-size: .72rem; letter-spacing: .04em; color: var(--fs); background: rgba(245,181,74,.1); border: 1px solid rgba(245,181,74,.22); padding: 4px 11px; border-radius: 999px; margin-bottom: 16px; }
  .ftag.m { color: var(--merge); background: rgba(154,160,255,.1); border-color: rgba(154,160,255,.22); }
  .section-title.narrow { max-width: 28ch; }
  .section-lead .muted { color: var(--text-faint); }

  .limits-note { margin-top: 34px; border: 1px solid var(--border); border-radius: 13px; background: var(--bg-card); padding: 18px 20px; }
  .limits-note h4 { margin: 0 0 12px; font-size: .82rem; font-family: var(--mono); color: var(--text-faint); font-weight: 600; letter-spacing: .05em; text-transform: uppercase; }
  .lpill { display: inline-flex; align-items: center; gap: 7px; margin: 0 8px 8px 0; font-family: var(--mono); font-size: .72rem; padding: 5px 11px; border-radius: 8px; border: 1px solid var(--border-soft); background: #0a0b0e; color: var(--text-dim); }
  .lpill .s { font-size: .62rem; padding: 2px 6px; border-radius: 5px; }
  .lpill .s.ok { background: rgba(74,210,149,.14); color: var(--ok); }
  .lpill .s.wn { background: rgba(245,181,74,.14); color: var(--fs); }
  .lpill .s.bd { background: rgba(242,97,106,.14); color: var(--danger); }

  .start-cta { display: flex; gap: 13px; flex-wrap: wrap; margin-top: 34px; }

  footer { border-top: 1px solid var(--border); padding: 40px 0; margin-top: 40px; }
  .foot-inner { display: flex; flex-wrap: wrap; gap: 24px; align-items: center; justify-content: space-between; }
  .foot-brand { display: flex; align-items: center; gap: 12px; }
  .foot-brand strong { font-family: var(--mono); display: block; }
  .foot-brand span { color: var(--text-dim); font-size: 0.85rem; }
  .foot-brand .brand-mark { width: 30px; height: 30px; }
  .foot-links { display: flex; gap: 18px; }
  .foot-links a { color: var(--text-dim); font-size: 0.9rem; }
  .foot-links a:hover { color: var(--text); }
  .foot-bottom { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; justify-content: space-between; margin-top: 26px; border-top: 1px solid var(--border-soft); padding-top: 20px; }
  .foot-license { color: var(--text-faint); font-size: 0.82rem; }
  .foot-legal-link { color: var(--text-dim); font-size: 0.82rem; font-family: var(--mono); }
  .foot-legal-link:hover { color: var(--text); }
</style>
