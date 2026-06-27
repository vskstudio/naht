<script>
  import logo from '../../assets/logo.png'
  import Icon from '../Icon.svelte'
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
    <Icon name={menuOpen ? 'x' : 'menu'} size={20} />
  </button>
  <a class="brand" href="#/"><img src={logo} alt="" width="24" height="24" /> naht</a>
  <span class="divider"></span>
  <span class="crumb"><Icon name="file" size={14} /> Docs</span>
  <button class="search" on:click={onSearch} aria-label="Search">
    <Icon name="search" size={15} />
    <span>Search</span><kbd>Ctrl K</kbd>
  </button>
  <nav>
    <a href="#/"><Icon name="home" size={15} /> <span>Home</span></a>
    <a href="#/docs/quickstart"><Icon name="file" size={15} /> <span>Docs</span></a>
    <a href={REPO} target="_blank" rel="noreferrer"><Icon name="github" size={15} /> <span>GitHub</span></a>
  </nav>
</header>

<style>
  .topbar {
    position: sticky; top: 0; z-index: 40; display: flex; align-items: center; gap: 14px;
    height: 56px; padding: 0 22px; border-bottom: 1px solid var(--border);
    background: rgba(7, 8, 9, 0.72); backdrop-filter: blur(12px);
  }
  .hamburger {
    display: none; align-items: center; justify-content: center;
    width: 36px; height: 36px; background: none; border: 1px solid transparent;
    border-radius: 7px; cursor: pointer; color: var(--text-dim); flex-shrink: 0;
  }
  .hamburger:hover { background: var(--bg-soft); border-color: var(--border); color: var(--text); }
  .brand { display: inline-flex; align-items: center; gap: 8px; font-family: var(--mono); font-weight: 700; color: var(--text); }
  .brand img { display: block; }
  .divider { width: 1px; height: 20px; background: var(--border); }
  .crumb {
    display: inline-flex; align-items: center; gap: 7px;
    color: var(--text-dim); font-size: 0.9rem;
  }
  .crumb :global(svg) { opacity: 0.6; }
  .search {
    display: inline-flex; align-items: center; gap: 9px; margin-left: 10px;
    color: var(--text-faint); background: var(--bg-soft); border: 1px solid var(--border);
    border-radius: 8px; padding: 6px 12px; font-size: 0.85rem; cursor: pointer; transition: border-color 0.15s ease;
  }
  .search :global(svg) { opacity: 0.7; }
  .search:hover { border-color: var(--border-strong); }
  .search kbd { font-family: var(--mono); font-size: 0.72rem; border: 1px solid var(--border); border-radius: 4px; padding: 1px 5px; }
  nav { margin-left: auto; display: flex; gap: 6px; }
  nav a {
    display: inline-flex; align-items: center; gap: 7px;
    color: var(--text-dim); font-size: 0.9rem; padding: 6px 11px; border-radius: 8px;
  }
  nav a :global(svg) { opacity: 0.7; }
  nav a:hover { color: var(--text); background: var(--bg-soft); }
  @media (max-width: 720px) {
    .topbar { gap: 10px; padding: 0 14px; }
    .search span, .search kbd { display: none; }
    .search { margin-left: auto; padding: 7px 9px; }
    .crumb, .divider { display: none; }
    .hamburger { display: inline-flex; }
    nav { margin-left: 0; }
    nav a span { display: none; }
    nav a { padding: 6px 8px; }
  }
</style>
