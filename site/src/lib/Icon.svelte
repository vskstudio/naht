<script>
  // A tiny inline-SVG icon set (stroke-based, 24×24, currentColor). No icon-font / network dependency.
  let { name, size = 22, stroke = 2 } = $props()

  const paths = {
    folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
    cube: 'M21 7.5 12 3 3 7.5m18 0L12 12m9-4.5V16.5L12 21m0-9v9m0-9L3 7.5M12 21 3 16.5V7.5',
    sync: 'M4 11a8 8 0 0 1 14-5l2 2m0 0V3m0 5h-5M20 13a8 8 0 0 1-14 5l-2-2m0 0v5m0-5h5',
    merge: 'M6 3v12a4 4 0 0 0 4 4h8m0 0-3-3m3 3-3 3M6 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4m12 12a2 2 0 1 0 0 4 2 2 0 0 0 0-4',
    shield: 'M12 3 5 6v5c0 4.5 3 7.5 7 9 4-1.5 7-4.5 7-9V6z M9.5 12l1.8 1.8L15 10',
    database: 'M12 3c4.4 0 8 1.3 8 3s-3.6 3-8 3-8-1.3-8-3 3.6-3 8-3m8 3v6c0 1.7-3.6 3-8 3s-8-1.3-8-3V6m16 6v6c0 1.7-3.6 3-8 3s-8-1.3-8-3v-6',
    bolt: 'M13 2 4 14h7l-1 8 9-12h-7z',
    terminal: 'M4 5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2zM7 9l3 3-3 3m5 0h5',
    warn: 'M12 3 2 20h20zM12 9v5m0 3h.01',
    plug: 'M9 2v6m6-6v6M6 8h12v3a6 6 0 0 1-12 0zm6 9v5',
    layers: 'M12 3 2 8.5 12 14l10-5.5zM2 13.5 12 19l10-5.5M2 8.5v5m20-5v5',
    git: 'M12 3v7m0 4v7M5 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6m0 0v3a4 4 0 0 0 4 4h3m7-3a3 3 0 1 1 0 6 3 3 0 0 1 0-6m0 0V7',
    check: 'M4 12.5 9 17.5 20 6.5',
    arrow: 'M5 12h14m-6-7 7 7-7 7',
    package: 'M21 8 12 3 3 8m18 0-9 5m9-5v8l-9 5m0-13L3 8m9 5v8M3 8v8l9 5',
    code: 'M8 6 2 12l6 6m8-12 6 6-6 6',
    spark: 'M12 2v5m0 10v5M2 12h5m10 0h5M5 5l3 3m8 8 3 3M19 5l-3 3M8 16l-3 3',
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
