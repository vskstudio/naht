<script>
  import Icon from './Icon.svelte'
  // Pure presentational: an animated schematic of the bidirectional sync. The amber stream is
  // filesystem → Studio; the cyan stream is Studio → filesystem; the violet core is naht-core +
  // the persisted SQLite base that powers the 3-way merge.
</script>

<div class="flow" role="img" aria-label="Filesystem and Roblox Studio sync bidirectionally through the naht daemon, which keeps a persisted base in SQLite for 3-way merges.">
  <!-- Filesystem node -->
  <div class="node fs">
    <div class="badge"><Icon name="folder" size={26} /></div>
    <strong>Filesystem</strong>
    <span><code>*.luau</code> on disk</span>
  </div>

  <!-- Wire + traveling packets, FS ↔ daemon -->
  <div class="wire left">
    <span class="dot fs d1"></span>
    <span class="dot fs d2"></span>
    <span class="dot studio r1"></span>
  </div>

  <!-- The daemon / core -->
  <div class="node core">
    <div class="core-glow"></div>
    <div class="badge core-badge"><Icon name="sync" size={26} /></div>
    <strong>naht daemon</strong>
    <span>reconcile · 3-way merge</span>
    <div class="base">
      <Icon name="database" size={14} />
      <code>.naht/state.db</code>
    </div>
  </div>

  <!-- Wire + traveling packets, daemon ↔ Studio -->
  <div class="wire right">
    <span class="dot fs d3"></span>
    <span class="dot studio r2"></span>
    <span class="dot studio r3"></span>
  </div>

  <!-- Studio node -->
  <div class="node studio">
    <div class="badge"><Icon name="cube" size={26} /></div>
    <strong>Roblox Studio</strong>
    <span>live DataModel</span>
  </div>
</div>

<div class="legend">
  <span class="key"><i class="sw fs"></i> Filesystem → Studio</span>
  <span class="key"><i class="sw studio"></i> Studio → Filesystem</span>
  <span class="key"><i class="sw merge"></i> Persisted base (SQLite)</span>
</div>

<style>
  .flow {
    width: 100%;
    max-width: 560px;
    margin-inline: auto;
    display: grid;
    grid-template-columns: 1fr auto 1.25fr auto 1fr;
    align-items: center;
    gap: 0;
    background: linear-gradient(180deg, var(--bg-soft), var(--bg-card));
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 38px 26px;
    box-shadow: var(--shadow);
  }

  .node {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    text-align: center;
    position: relative;
  }
  .node strong {
    font-size: 1rem;
    letter-spacing: -0.01em;
  }
  .node span {
    font-size: 0.8rem;
    color: var(--text-dim);
  }

  .badge {
    width: 60px;
    height: 60px;
    display: grid;
    place-items: center;
    border-radius: 16px;
    margin-bottom: 6px;
    border: 1px solid var(--border);
  }
  .fs .badge {
    color: var(--fs);
    background: var(--fs-soft);
    box-shadow: 0 0 0 1px rgba(245, 181, 74, 0.25);
  }
  .studio .badge {
    color: var(--studio);
    background: var(--studio-soft);
    box-shadow: 0 0 0 1px rgba(69, 200, 224, 0.25);
  }

  .core {
    padding: 18px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: rgba(139, 124, 246, 0.06);
    position: relative;
    overflow: hidden;
  }
  .core-glow {
    position: absolute;
    inset: -40% 30%;
    background: radial-gradient(circle, rgba(139, 124, 246, 0.35), transparent 65%);
    animation: pulse 3.6s ease-in-out infinite;
  }
  .core-badge {
    color: var(--merge);
    background: rgba(139, 124, 246, 0.14);
    box-shadow: 0 0 0 1px rgba(139, 124, 246, 0.3);
    animation: spin 9s linear infinite;
    position: relative;
    z-index: 1;
  }
  .core strong,
  .core span,
  .base {
    position: relative;
    z-index: 1;
  }
  .base {
    margin-top: 10px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 9px;
    border-radius: 999px;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--merge);
    font-size: 0.72rem;
  }
  .base code {
    color: var(--text-dim);
  }

  /* Connector wires with two animated rails of packets. */
  .wire {
    position: relative;
    height: 3px;
    margin: 0 -2px;
    align-self: center;
    background: linear-gradient(90deg, var(--border-soft), var(--border), var(--border-soft));
    border-radius: 3px;
    min-width: 70px;
  }

  .dot {
    position: absolute;
    top: 50%;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    margin-top: -4.5px;
    filter: drop-shadow(0 0 5px currentColor);
  }
  .dot.fs {
    background: var(--fs);
    color: var(--fs);
  }
  .dot.studio {
    background: var(--studio);
    color: var(--studio);
  }

  /* FS → Studio packets travel left→right; Studio → FS travel right→left. Staggered delays so the
     two streams read as continuous, independent flows. */
  .left .d1 {
    animation: ltr 2.4s linear infinite;
  }
  .left .d2 {
    animation: ltr 2.4s linear infinite 1.2s;
  }
  .left .r1 {
    animation: rtl 2.4s linear infinite 0.6s;
  }
  .right .d3 {
    animation: ltr 2.4s linear infinite 0.3s;
  }
  .right .r2 {
    animation: rtl 2.4s linear infinite 1.5s;
  }
  .right .r3 {
    animation: rtl 2.4s linear infinite 0.9s;
  }

  @keyframes ltr {
    from {
      left: -4px;
      opacity: 0;
    }
    12%,
    88% {
      opacity: 1;
    }
    to {
      left: calc(100% - 4px);
      opacity: 0;
    }
  }
  @keyframes rtl {
    from {
      left: calc(100% - 4px);
      opacity: 0;
    }
    12%,
    88% {
      opacity: 1;
    }
    to {
      left: -4px;
      opacity: 0;
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 0.5;
      transform: scale(0.9);
    }
    50% {
      opacity: 1;
      transform: scale(1.1);
    }
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 22px;
    margin-top: 22px;
  }
  .key {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 0.82rem;
    color: var(--text-dim);
  }
  .sw {
    width: 13px;
    height: 13px;
    border-radius: 4px;
    display: inline-block;
  }
  .sw.fs {
    background: var(--fs);
  }
  .sw.studio {
    background: var(--studio);
  }
  .sw.merge {
    background: var(--merge);
  }

  @media (max-width: 720px) {
    .flow {
      grid-template-columns: 1fr;
      gap: 10px;
      padding: 26px 18px;
    }
    .wire {
      width: 3px;
      height: 46px;
      min-width: 0;
      margin: 0 auto;
      background: linear-gradient(180deg, var(--border-soft), var(--border), var(--border-soft));
    }
    .dot {
      left: 50% !important;
      margin-left: -4.5px;
      margin-top: 0;
    }
    .left .d1 {
      animation: ttb 2.4s linear infinite;
    }
    .left .d2 {
      animation: ttb 2.4s linear infinite 1.2s;
    }
    .left .r1 {
      animation: btt 2.4s linear infinite 0.6s;
    }
    .right .d3 {
      animation: ttb 2.4s linear infinite 0.3s;
    }
    .right .r2 {
      animation: btt 2.4s linear infinite 1.5s;
    }
    .right .r3 {
      animation: btt 2.4s linear infinite 0.9s;
    }
    @keyframes ttb {
      from {
        top: -4px;
        opacity: 0;
      }
      12%,
      88% {
        opacity: 1;
      }
      to {
        top: calc(100% - 4px);
        opacity: 0;
      }
    }
    @keyframes btt {
      from {
        top: calc(100% - 4px);
        opacity: 0;
      }
      12%,
      88% {
        opacity: 1;
      }
      to {
        top: -4px;
        opacity: 0;
      }
    }
  }
</style>
