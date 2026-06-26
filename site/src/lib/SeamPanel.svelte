<script>
  // Animated DISK ⇄ STUDIO "seam": a stitched centre line with a file label
  // travelling both ways. Pure CSS/SVG. Motion is disabled under reduced-motion.
</script>

<div class="seam" role="img" aria-label="A file syncing both ways between disk and Roblox Studio">
  <div class="status"><span class="led"></span> connected · re-diffed on connect</div>
  <div class="cols">
    <div class="mini">
      <div class="lab">DISK</div>
      <pre>src/
  Sword.luau
  Hitbox.luau</pre>
    </div>
    <div class="seam-line" aria-hidden="true">
      <span class="stitch"></span>
      <span class="pkt">Sword.luau</span>
    </div>
    <div class="mini">
      <div class="lab">STUDIO</div>
      <pre>ReplicatedStorage
  Sword
  Hitbox</pre>
    </div>
  </div>
  <div class="foot">⇄ synced both ways</div>
</div>

<style>
  .seam {
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--bg-card);
    box-shadow: var(--shadow);
    padding: 18px;
  }
  .status {
    display: flex; align-items: center; gap: 8px;
    font-family: var(--mono); font-size: 0.74rem; color: var(--text-faint);
    margin-bottom: 16px;
  }
  .led { width: 8px; height: 8px; border-radius: 50%; background: var(--ok);
    box-shadow: 0 0 8px var(--ok); }
  .cols { position: relative; display: grid; grid-template-columns: 1fr auto 1fr; gap: 18px; }
  .mini { border: 1px solid var(--border-strong); border-radius: var(--radius-sm);
    background: var(--bg-soft); padding: 12px; }
  .lab { font-family: var(--mono); font-size: 0.66rem; letter-spacing: 0.08em;
    color: var(--text-faint); margin-bottom: 8px; }
  pre { margin: 0; font-family: var(--mono); font-size: 0.78rem; line-height: 1.7;
    color: var(--text-dim); white-space: pre; }
  .seam-line { position: relative; width: 2px; align-self: stretch; }
  .stitch { position: absolute; inset: 0; width: 2px; left: 0;
    background: repeating-linear-gradient(var(--studio) 0 6px, transparent 6px 12px);
    opacity: 0.7; animation: stitch 1.1s linear infinite; }
  .pkt {
    position: absolute; left: 50%; top: 50%;
    transform: translate(-50%, -50%);
    font-family: var(--mono); font-size: 0.6rem; color: #06202b;
    background: var(--studio); border-radius: 999px; padding: 2px 7px; white-space: nowrap;
    animation: travel 3.2s ease-in-out infinite;
  }
  .foot { text-align: center; margin-top: 14px; font-family: var(--mono);
    font-size: 0.72rem; color: var(--studio); }
  @keyframes stitch { to { background-position: 0 12px; } }
  @keyframes travel {
    0%, 100% { transform: translate(-180px, -50%); opacity: 0; }
    15%, 35% { opacity: 1; }
    50% { transform: translate(-50%, -50%); }
    65%, 85% { opacity: 1; }
    50.001% { transform: translate(120px, -50%); }
  }
  @media (max-width: 720px) {
    .cols { grid-template-columns: 1fr; }
    .seam-line { width: auto; height: 2px; margin: 4px 0; }
    .stitch { width: 100%; height: 2px;
      background: repeating-linear-gradient(90deg, var(--studio) 0 6px, transparent 6px 12px); }
  }
  @media (prefers-reduced-motion: reduce) {
    .stitch { animation: none; }
    .pkt { animation: none; transform: translate(-50%, -50%); }
  }
</style>
