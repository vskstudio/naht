<script>
  import Icon from './Icon.svelte'
  // Two side-by-side outcomes of a concurrent edit: a clean 3-way merge (both edits kept) and an
  // unmergeable conflict (git-style markers, path frozen). Mirrors reconciler::reconcile.
</script>

<div class="grid">
  <!-- Clean merge -->
  <div class="panel ok">
    <header>
      <span class="tag ok"><Icon name="merge" size={15} /> Clean 3-way merge</span>
    </header>
    <div class="base-row">
      <span class="chip base">base</span>
      <code>local hp = 100</code>
    </div>
    <div class="branches">
      <div class="branch fs">
        <span class="chip fs">disk</span>
        <code>local <b>hp = 120</b></code>
      </div>
      <div class="branch studio">
        <span class="chip studio">studio</span>
        <code>local hp = 100<br /><b>local mp = 50</b></code>
      </div>
    </div>
    <div class="join"><Icon name="arrow" size={18} /></div>
    <div class="result ok">
      <span class="chip merged"><Icon name="check" size={13} /> merged</span>
      <code>local <b>hp = 120</b><br /><b>local mp = 50</b></code>
    </div>
    <p class="note">Non-overlapping edits combine. Base advances; nothing is lost.</p>
  </div>

  <!-- Conflict -->
  <div class="panel bad">
    <header>
      <span class="tag bad"><Icon name="warn" size={15} /> Unmergeable → frozen</span>
    </header>
    <div class="base-row">
      <span class="chip base">base</span>
      <code>local hp = 100</code>
    </div>
    <div class="branches">
      <div class="branch fs">
        <span class="chip fs">disk</span>
        <code>local hp = <b>120</b></code>
      </div>
      <div class="branch studio">
        <span class="chip studio">studio</span>
        <code>local hp = <b>80</b></code>
      </div>
    </div>
    <div class="join"><Icon name="arrow" size={18} /></div>
    <div class="result bad">
      <span class="chip frozen"><Icon name="git" size={13} /> conflict · frozen</span>
      <pre><span class="m">&lt;&lt;&lt;&lt;&lt;&lt;&lt; FS</span>
local hp = 120
<span class="m">=======</span>
local hp = 80
<span class="m">&gt;&gt;&gt;&gt;&gt;&gt;&gt; Studio</span></pre>
    </div>
    <p class="note">Same line, both sides: markers written, path frozen. <code>naht resolve</code> clears it.</p>
  </div>
</div>

<style>
  .grid {
    width: 100%;
    max-width: 560px;
    margin-inline: auto;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
  }
  .panel {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .panel.ok {
    box-shadow: inset 0 0 0 1px rgba(74, 210, 149, 0.1);
  }
  .panel.bad {
    box-shadow: inset 0 0 0 1px rgba(242, 97, 106, 0.1);
  }

  header {
    display: flex;
    align-items: center;
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 0.85rem;
    font-weight: 600;
    padding: 5px 11px;
    border-radius: 999px;
  }
  .tag.ok {
    color: var(--ok);
    background: rgba(74, 210, 149, 0.1);
  }
  .tag.bad {
    color: var(--danger);
    background: rgba(242, 97, 106, 0.1);
  }

  code,
  pre {
    font-family: var(--mono);
    font-size: 0.78rem;
    color: var(--text-dim);
    margin: 0;
  }
  code b {
    color: var(--text);
    font-weight: 600;
  }
  pre {
    white-space: pre;
    overflow-x: auto;
    line-height: 1.5;
  }
  pre .m {
    color: var(--merge);
  }

  .base-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    background: var(--bg-soft);
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-sm);
  }

  .branches {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .branch {
    padding: 9px 12px;
    border-radius: var(--radius-sm);
    background: var(--bg-soft);
    border: 1px solid var(--border-soft);
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .branch.fs {
    border-color: rgba(245, 181, 74, 0.3);
  }
  .branch.studio {
    border-color: rgba(69, 200, 224, 0.3);
  }

  .chip {
    align-self: flex-start;
    font-family: var(--mono);
    font-size: 0.64rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 3px 8px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .chip.base {
    background: var(--border);
    color: var(--text-dim);
  }
  .chip.fs {
    background: var(--fs-soft);
    color: var(--fs);
  }
  .chip.studio {
    background: var(--studio-soft);
    color: var(--studio);
  }
  .chip.merged {
    background: rgba(74, 210, 149, 0.12);
    color: var(--ok);
  }
  .chip.frozen {
    background: rgba(242, 97, 106, 0.12);
    color: var(--danger);
  }

  .join {
    display: grid;
    place-items: center;
    color: var(--text-faint);
    transform: rotate(90deg);
  }

  .result {
    padding: 11px 13px;
    border-radius: var(--radius-sm);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .result.ok {
    background: rgba(74, 210, 149, 0.06);
    border: 1px solid rgba(74, 210, 149, 0.25);
  }
  .result.bad {
    background: rgba(242, 97, 106, 0.06);
    border: 1px solid rgba(242, 97, 106, 0.25);
  }

  .note {
    margin: 2px 0 0;
    font-size: 0.82rem;
    color: var(--text-faint);
  }
  .note code {
    color: var(--studio);
    font-size: 0.78rem;
  }

  @media (max-width: 720px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
