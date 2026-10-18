<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  interface Props {
    open: boolean;
    title: string;
    /** Called on Esc, the close button or a click outside. */
    onclose: () => void;
    width?: number;
    children: Snippet;
    actions?: Snippet;
  }

  let { open, title, onclose, width = 440, children, actions }: Props = $props();

  let box: HTMLDivElement | undefined = $state();
  const titleId = `dlg-${Math.random().toString(36).slice(2, 9)}`;

  $effect(() => {
    if (!open || !box) return;
    const before = document.activeElement as HTMLElement | null;
    // Focus the first control, else the box itself.
    const first = box.querySelector<HTMLElement>('[data-autofocus], button:not([disabled]), input, select, textarea');
    (first ?? box).focus();
    return () => before?.focus?.();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
      return;
    }
    if (e.key !== 'Tab' || !box) return;
    // Keep Tab inside the dialog.
    const items = Array.from(box.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), select, textarea, [tabindex="0"]'));
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

{#if open}
  <div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
    <div
      class="box"
      bind:this={box}
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      tabindex="-1"
      style:width="{width}px"
      {onkeydown}
    >
      <header>
        <h2 id={titleId}>{title}</h2>
        <button type="button" class="x" aria-label="닫기" onclick={onclose}><Icon name="close" size={18} /></button>
      </header>
      <div class="body">{@render children()}</div>
      {#if actions}<footer>{@render actions()}</footer>{/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: var(--s5);
    background: var(--overlay);
  }
  .box {
    max-width: 100%;
    max-height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }
  .box:focus-visible {
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: var(--s4) var(--s4) var(--s2) var(--s5);
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .x {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .x:hover {
    background: var(--panel-2);
    color: var(--text);
  }
  .body {
    padding: var(--s2) var(--s5) var(--s4);
    overflow: auto;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--s2);
    padding: var(--s3) var(--s5) var(--s4);
    border-top: 1px solid var(--border);
  }
</style>
