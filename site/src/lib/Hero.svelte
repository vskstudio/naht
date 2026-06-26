<script>
  import Icon from './Icon.svelte'
  import { reveal } from './reveal.js'
  import SeamPanel from './SeamPanel.svelte'
</script>

<header id="top" class="hero">
  <div class="wrap hero-grid">
    <div class="hero-copy">
      <span class="pill reveal" use:reveal>
        <Icon name="spark" size={14} /> Rust · Roblox Studio · bidirectional sync
      </span>
      <h1 class="reveal" use:reveal={{ delay: 60 }}>
        The <span class="grad">seam</span> between your filesystem<br />and Roblox Studio.
      </h1>
      <p class="lead reveal" use:reveal={{ delay: 120 }}>
        Bidirectional sync with a real 3-way merge. Never destructive.
      </p>
      <div class="cta reveal" use:reveal={{ delay: 180 }}>
        <a class="btn primary" href="#start"><Icon name="bolt" size={17} /> Quickstart</a>
        <a class="btn ghost" href="#sync"><Icon name="sync" size={17} /> See it sync</a>
      </div>
    </div>
    <div class="hero-visual reveal" use:reveal={{ delay: 160 }}>
      <SeamPanel />
    </div>
  </div>
</header>

<style>
  .hero { padding: 96px 0 72px; position: relative; }
  .hero-grid {
    display: grid; grid-template-columns: 1fr 1.05fr; gap: 44px; align-items: center;
  }
  .pill {
    display: inline-flex; align-items: center; gap: 8px;
    font-family: var(--mono); font-size: 0.76rem; letter-spacing: 0.04em;
    color: var(--fs); padding: 7px 15px; border: 1px solid var(--border-strong);
    border-radius: 999px; background: var(--bg-soft); margin-bottom: 24px;
  }
  .hero-copy { min-width: 0; }
  h1 {
    font-size: clamp(2.1rem, 5vw, 3.7rem);
    letter-spacing: -0.02em; line-height: 1.08; margin: 0 0 18px;
  }
  .lead { color: var(--text-dim); font-size: 1.1rem; max-width: 44ch; margin: 0 0 28px; }
  .cta { display: flex; gap: 13px; flex-wrap: wrap; }
  @media (max-width: 720px) {
    .hero { padding: 60px 0 48px; text-align: center; }
    .hero-grid { grid-template-columns: 1fr; gap: 32px; }
    .pill, .cta { justify-content: center; }
    .lead { margin-left: auto; margin-right: auto; }
  }
</style>
