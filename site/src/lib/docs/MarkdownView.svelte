<script>
  import { renderDoc } from './markdown.js'
  import { docs } from './nav.js'

  export let slug
  export let toc = []   // bindable: parent reads the active doc's TOC

  $: doc = docs[slug]
  $: rendered = doc ? renderDoc(doc.raw) : { html: '', toc: [] }
  $: toc = rendered.toc
</script>

{#if doc}
  <article class="prose">{@html rendered.html}</article>
{:else}
  <article class="prose"><h2>Not found</h2><p>No doc named "{slug}".</p></article>
{/if}

<style>
  .prose { max-width: 76ch; color: var(--text); line-height: 1.7; }
  .prose :global(h1) { font-size: 2rem; letter-spacing: -0.02em; margin: 0 0 18px; }
  .prose :global(h2) { font-size: 1.4rem; margin: 38px 0 12px; letter-spacing: -0.01em; }
  .prose :global(h3) { font-size: 1.1rem; margin: 26px 0 10px; }
  .prose :global(p), .prose :global(li) { color: var(--text-dim); }
  .prose :global(a) { color: var(--studio); text-decoration: none; }
  .prose :global(a:hover) { text-decoration: underline; }
  .prose :global(code) {
    font-family: var(--mono); font-size: 0.88em; color: var(--fs);
    background: var(--bg-soft); border: 1px solid var(--border); border-radius: 5px; padding: 1px 5px;
  }
  .prose :global(pre) {
    background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-sm);
    padding: 14px 16px; overflow-x: auto;
  }
  .prose :global(pre code) { background: none; border: none; color: var(--text); padding: 0; }
  .prose :global(blockquote) {
    border-left: 2px solid var(--border-strong); margin: 16px 0; padding: 2px 16px; color: var(--text-faint);
  }
  .prose :global(.table-wrap) { overflow-x: auto; margin: 16px 0; }
  .prose :global(table) { border-collapse: collapse; width: 100%; margin: 0; }
  .prose :global(th), .prose :global(td) {
    border: 1px solid var(--border); padding: 8px 12px; text-align: left; font-size: 0.92rem;
  }
  .prose :global(.callout) {
    border: 1px solid var(--border-strong); border-radius: var(--radius-sm);
    background: var(--bg-soft); padding: 12px 16px; margin: 18px 0;
  }
  .prose :global(.callout.note) { border-left: 3px solid var(--merge); }
  .prose :global(.callout.tip) { border-left: 3px solid var(--ok); }
  .prose :global(.callout.warning), .prose :global(.callout.important) { border-left: 3px solid var(--fs); }
  .prose :global(.callout > :first-child) { margin-top: 0; }
  .prose :global(.callout > :last-child) { margin-bottom: 0; }
</style>
