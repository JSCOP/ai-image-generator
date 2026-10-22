<script lang="ts">
  // Left icon navigation (64px): generate, gallery, connections. The editor (mask edit, background removal) opens over any screen.
  import type { Screen } from '../lib/types';
  import Icon, { type IconName } from './Icon.svelte';

  interface Props {
    screen: Screen;
    onnavigate: (screen: Screen) => void;
  }

  let { screen, onnavigate }: Props = $props();

  const ITEMS: { screen: Screen; icon: IconName; label: string }[] = [
    { screen: 'generate', icon: 'generate', label: '생성' },
    { screen: 'gallery', icon: 'gallery', label: '갤러리' },
    { screen: 'connections', icon: 'connections', label: '연결' },
  ];
</script>

<nav aria-label="화면">
  {#each ITEMS as item (item.screen)}
    <button
      type="button"
      class:on={item.screen === screen}
      aria-current={item.screen === screen ? 'page' : undefined}
      title={item.label}
      onclick={() => onnavigate(item.screen)}
    >
      <Icon name={item.icon} size={22} />
      <span>{item.label}</span>
    </button>
  {/each}
</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s2);
    padding: var(--s3) 0;
    background: var(--bg);
    border-right: 1px solid var(--border);
  }
  button {
    position: relative;
    width: 52px;
    height: 56px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-size: 11px;
  }
  button:hover {
    color: var(--text);
    background: var(--panel);
  }
  .on {
    color: #fff;
    background: var(--accent-soft);
  }
  .on::before {
    content: '';
    position: absolute;
    left: -6px;
    top: 12px;
    bottom: 12px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .on :global(svg) {
    color: #c4b5fd;
  }
</style>
