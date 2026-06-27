<script>
  import { onMount } from 'svelte'
  import { getDoc, getMeta } from './registry.js'
  import { order, meta as navMeta } from './nav.js'
  import Icon from '../Icon.svelte'

  export let slug
  export let locale = 'en'
  export let toc = []   // bindable: parent reads the active doc's TOC

  $: doc = getDoc(locale, slug)
  $: m = doc?.metadata ?? {}
  $: accent = m.accent ?? navMeta[slug]?.accent ?? 'fs'
  $: icon = m.icon ?? navMeta[slug]?.icon ?? 'file'

  // Prev / next within the flat reading order.
  $: idx = order.indexOf(slug)
  $: prev = idx > 0 ? order[idx - 1] : null
  $: next = idx >= 0 && idx < order.length - 1 ? order[idx + 1] : null

  let article
  const COPY = '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>'
  const CHECK = '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>'

  onMount(() => {
    if (!article) return
    // Build the right-rail TOC from rendered headings (ids come from rehype-slug).
    toc = [...article.querySelectorAll('h2, h3')]
      .filter((h) => h.id)
      .map((h) => ({ id: h.id, text: h.textContent, level: h.tagName === 'H2' ? 2 : 3 }))

    // Inject copy buttons into every code block.
    const cleanups = []
    article.querySelectorAll('pre').forEach((pre) => {
      if (pre.querySelector('.copy-btn')) return
      const btn = document.createElement('button')
      btn.className = 'copy-btn'
      btn.type = 'button'
      btn.setAttribute('aria-label', 'Copy code')
      btn.innerHTML = COPY
      const onClick = async () => {
        try {
          await navigator.clipboard.writeText(pre.querySelector('code')?.innerText ?? pre.innerText)
          btn.innerHTML = CHECK
          btn.classList.add('copied')
          setTimeout(() => { btn.innerHTML = COPY; btn.classList.remove('copied') }, 1400)
        } catch { /* clipboard blocked — no-op */ }
      }
      btn.addEventListener('click', onClick)
      pre.appendChild(btn)
      cleanups.push(() => btn.removeEventListener('click', onClick))
    })
    return () => cleanups.forEach((c) => c())
  })
</script>

{#if doc}
  <header class="doc-head accent-{accent}">
    <div class="badge"><Icon name={icon} size={20} stroke={1.8} /></div>
    <div>
      <h1>{m.title ?? slug}</h1>
      {#if m.desc}<p class="lead">{m.desc}</p>{/if}
    </div>
  </header>

  <article class="prose" bind:this={article}>
    <svelte:component this={doc.component} />
  </article>

  {#if prev || next}
    <nav class="pager">
      {#if prev}
        <a class="pager-card prev accent-{navMeta[prev]?.accent ?? 'fs'}" href={`#/docs/${prev}`}>
          <span class="dir"><Icon name="arrowLeft" size={14} /> {locale === 'fr' ? 'Précédent' : 'Previous'}</span>
          <span class="title"><Icon name={navMeta[prev]?.icon ?? 'file'} size={15} /> {getMeta(locale, prev).title ?? prev}</span>
        </a>
      {:else}<span></span>{/if}
      {#if next}
        <a class="pager-card next accent-{navMeta[next]?.accent ?? 'fs'}" href={`#/docs/${next}`}>
          <span class="dir">{locale === 'fr' ? 'Suivant' : 'Next'} <Icon name="arrowRight" size={14} /></span>
          <span class="title">{getMeta(locale, next).title ?? next} <Icon name={navMeta[next]?.icon ?? 'file'} size={15} /></span>
        </a>
      {:else}<span></span>{/if}
    </nav>
  {/if}
{:else}
  <article class="prose"><h2>Not found</h2><p>No doc named "{slug}".</p></article>
{/if}

<style>
  /* ---- Doc header ---- */
  .doc-head { display: flex; gap: 18px; align-items: flex-start; margin: 0 0 14px; max-width: 80ch; }
  .badge {
    flex-shrink: 0; display: grid; place-items: center; width: 46px; height: 46px;
    border-radius: 13px; color: var(--accent); margin-top: 4px;
    background: color-mix(in oklab, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in oklab, var(--accent) 32%, transparent);
    box-shadow: 0 0 36px -8px color-mix(in oklab, var(--accent) 50%, transparent);
  }
  .doc-head h1 { font-size: 2.15rem; letter-spacing: -0.025em; margin: 0; }
  .doc-head .lead { color: var(--text-dim); font-size: 1.06rem; line-height: 1.6; margin: 10px 0 0; }
  .accent-fs { --accent: var(--fs); }
  .accent-studio { --accent: var(--studio); }
  .accent-merge { --accent: var(--merge); }
  .doc-head + .prose { margin-top: 14px; }

  /* ---- Prose ---- */
  .prose { max-width: 80ch; color: var(--text); line-height: 1.85; font-size: 1.02rem; }
  .prose :global(h1) { display: none; }
  .prose :global(h2) {
    font-size: 1.45rem; margin: 68px 0 18px; letter-spacing: -0.01em;
    padding-top: 22px; border-top: 1px solid var(--border-soft);
  }
  .prose :global(h3) { font-size: 1.12rem; margin: 44px 0 14px; }
  .prose :global(p), .prose :global(li) { color: var(--text-dim); }
  .prose :global(p) { margin: 0 0 22px; }
  .prose :global(ul), .prose :global(ol) { margin: 0 0 22px; padding-left: 24px; }
  .prose :global(li) { margin: 10px 0; padding-left: 4px; }
  .prose :global(li::marker) { color: var(--text-faint); }
  .prose :global(li > ul), .prose :global(li > ol) { margin: 10px 0 0; }
  .prose :global(h2 + p), .prose :global(h3 + p) { margin-top: 0; }
  .prose :global(strong) { color: var(--text); font-weight: 650; }
  .prose :global(a) { color: var(--studio); text-decoration: none; border-bottom: 1px solid color-mix(in oklab, var(--studio) 35%, transparent); }
  .prose :global(a:hover) { border-bottom-color: var(--studio); }
  .prose :global(code) {
    font-family: var(--mono); font-size: 0.86em; color: var(--fs);
    background: var(--bg-soft); border: 1px solid var(--border); border-radius: 5px; padding: 1px 5px;
  }

  /* code blocks + copy button */
  .prose :global(pre) {
    position: relative;
    background: linear-gradient(180deg, #0c0e13, #090a0e);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    padding: 18px 20px; margin: 0 0 24px; overflow-x: auto; line-height: 1.65;
  }
  .prose :global(pre code) { background: none; border: none; color: #d7dbe3; padding: 0; font-size: 0.86rem; }
  .prose :global(.copy-btn) {
    position: absolute; top: 10px; right: 10px; display: grid; place-items: center;
    width: 30px; height: 30px; border-radius: 8px; cursor: pointer;
    color: var(--text-faint); background: rgba(20, 23, 30, 0.8);
    border: 1px solid var(--border); opacity: 0; transition: opacity 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }
  .prose :global(pre:hover .copy-btn) { opacity: 1; }
  .prose :global(.copy-btn:hover) { color: var(--text); border-color: var(--border-strong); }
  .prose :global(.copy-btn.copied) { color: var(--ok); border-color: color-mix(in oklab, var(--ok) 45%, transparent); opacity: 1; }

  .prose :global(blockquote) {
    border-left: 2px solid var(--border-strong); margin: 24px 0; padding: 4px 18px; color: var(--text-faint);
  }
  .prose :global(table) { border-collapse: collapse; width: 100%; margin: 24px 0; display: block; overflow-x: auto; }
  .prose :global(th), .prose :global(td) {
    border-bottom: 1px solid var(--border); padding: 10px 14px; text-align: left; font-size: 0.92rem;
  }
  .prose :global(th) {
    background: var(--bg-soft); color: var(--text); font-family: var(--mono);
    font-size: 0.78rem; letter-spacing: 0.04em; text-transform: uppercase; font-weight: 600; white-space: nowrap;
  }
  .prose :global(tr:last-child td) { border-bottom: none; }

  /* ---- Prev / next pager ---- */
  .pager {
    display: grid; grid-template-columns: 1fr 1fr; gap: 16px;
    margin: 64px 0 0; padding-top: 28px; border-top: 1px solid var(--border-soft); max-width: 80ch;
  }
  .pager-card {
    display: flex; flex-direction: column; gap: 8px; padding: 16px 18px;
    border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg-soft);
    transition: border-color 0.15s ease, transform 0.15s ease;
  }
  .pager-card.next { text-align: right; align-items: flex-end; }
  .pager-card:hover { border-color: color-mix(in oklab, var(--accent) 55%, var(--border-strong)); transform: translateY(-2px); }
  .pager-card.accent-fs { --accent: var(--fs); }
  .pager-card.accent-studio { --accent: var(--studio); }
  .pager-card.accent-merge { --accent: var(--merge); }
  .pager-card .dir {
    display: inline-flex; align-items: center; gap: 6px;
    font-family: var(--mono); font-size: 0.72rem; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-faint);
  }
  .pager-card .title { display: inline-flex; align-items: center; gap: 8px; color: var(--text); font-weight: 600; }
  .pager-card .title :global(svg) { color: var(--accent); }
  @media (max-width: 560px) {
    .pager { grid-template-columns: 1fr; }
    .doc-head { gap: 12px; }
    .doc-head h1 { font-size: 1.6rem; }
    .doc-head .lead { font-size: 0.98rem; }
    .doc-head .badge { width: 38px; height: 38px; }
    .prose { font-size: 0.98rem; }
    .prose :global(h2) { font-size: 1.28rem; margin: 48px 0 14px; }
  }
</style>
