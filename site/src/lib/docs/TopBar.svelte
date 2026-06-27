<script>
  import logo from '../../assets/logo.png'
  const REPO = 'https://github.com/vskstudio/naht'
  export let onSearch = () => {}
  export let onMenu = () => {}
  export let menuOpen = false
</script>

<header class="topbar">
  <button
    class="hamburger"
    on:click={onMenu}
    aria-label={menuOpen ? 'Close menu' : 'Open menu'}
    aria-expanded={menuOpen}
  >
    <span class="bar"></span>
    <span class="bar"></span>
    <span class="bar"></span>
  </button>
  <a class="brand" href="#/"><img src={logo} alt="" width="24" height="24" /> naht</a>
  <button class="search" on:click={onSearch} aria-label="Search">
    <span>Search</span><kbd>Ctrl K</kbd>
  </button>
  <nav>
    <a href="#/">Home</a>
    <a href="#/docs/quickstart">Docs</a>
    <a href={REPO} target="_blank" rel="noreferrer">GitHub</a>
  </nav>
</header>

<style>
  .topbar {
    position: sticky; top: 0; z-index: 40; display: flex; align-items: center; gap: 16px;
    height: 56px; padding: 0 20px; border-bottom: 1px solid var(--border);
    background: rgba(7, 8, 9, 0.72); backdrop-filter: blur(12px);
  }
  .hamburger {
    display: none;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    gap: 5px;
    width: 36px; height: 36px;
    background: none;
    border: 1px solid transparent;
    border-radius: 7px;
    cursor: pointer;
    padding: 7px;
    flex-shrink: 0;
  }
  .hamburger:hover { background: var(--bg-soft); border-color: var(--border); }
  .bar { display: block; width: 18px; height: 2px; background: var(--text-dim); border-radius: 2px; }
  .brand { display: inline-flex; align-items: center; gap: 8px; font-family: var(--mono); font-weight: 700; color: var(--text); }
  .brand img { image-rendering: pixelated; display: block; }
  .search {
    display: inline-flex; align-items: center; gap: 10px; margin-left: 8px;
    color: var(--text-faint); background: var(--bg-soft); border: 1px solid var(--border);
    border-radius: 8px; padding: 6px 12px; font-size: 0.85rem; cursor: pointer;
  }
  .search:hover { border-color: var(--border-strong); }
  .search kbd { font-family: var(--mono); font-size: 0.72rem; border: 1px solid var(--border); border-radius: 4px; padding: 1px 5px; }
  nav { margin-left: auto; display: flex; gap: 18px; }
  nav a { color: var(--text-dim); font-size: 0.9rem; }
  nav a:hover { color: var(--text); }
  @media (max-width: 720px) {
    .search span { display: none; }
    .hamburger { display: flex; }
  }
</style>
