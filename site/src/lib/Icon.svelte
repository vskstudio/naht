<script>
  // A tiny inline-SVG icon set (stroke-based, 24×24, currentColor). No icon-font / network dependency.
  let { name, size = 22, stroke = 2 } = $props()

  const paths = {
    sync: 'M4 11a8 8 0 0 1 14-5l2 2m0 0V3m0 5h-5M20 13a8 8 0 0 1-14 5l-2-2m0 0v5m0-5h5',
    layers: 'M12 3 2 8.5 12 14l10-5.5zM2 13.5 12 19l10-5.5M2 8.5v5m20-5v5',
    git: 'M12 3v7m0 4v7M5 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6m0 0v3a4 4 0 0 0 4 4h3m7-3a3 3 0 1 1 0 6 3 3 0 0 1 0-6m0 0V7',
    arrow: 'M5 12h14m-6-7 7 7-7 7',
  }
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name] ?? ''} />
</svg>
