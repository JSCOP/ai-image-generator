<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon, { type IconName } from './Icon.svelte';

  interface Props {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
    size?: 'md' | 'sm' | 'lg';
    type?: 'button' | 'submit';
    icon?: IconName;
    disabled?: boolean;
    full?: boolean;
    title?: string;
    /** For icon-only buttons. */
    label?: string;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  }

  let {
    variant = 'secondary',
    size = 'md',
    type = 'button',
    icon,
    disabled = false,
    full = false,
    title,
    label,
    onclick,
    children,
  }: Props = $props();
</script>

<button
  {type}
  class="btn {variant} {size}"
  class:full
  class:icon-only={!children}
  {disabled}
  {title}
  aria-label={label}
  {onclick}
>
  {#if icon}<Icon name={icon} size={size === 'sm' ? 16 : 18} />{/if}
  {#if children}<span>{@render children()}</span>{/if}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--s2);
    height: 36px;
    padding: 0 var(--s4);
    border: 1px solid transparent;
    border-radius: var(--radius);
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
    transition:
      background-color 0.12s,
      border-color 0.12s,
      color 0.12s;
  }
  .sm {
    height: 30px;
    padding: 0 var(--s3);
    font-size: 13px;
  }
  .lg {
    height: 48px;
    font-size: 15px;
  }
  .icon-only {
    padding: 0;
    width: 36px;
  }
  .icon-only.sm {
    width: 30px;
  }
  .full {
    width: 100%;
  }
  .primary {
    background: var(--accent);
    color: #fff;
  }
  .primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .secondary {
    background: var(--panel-2);
    border-color: var(--border);
    color: var(--text);
  }
  .secondary:hover:not(:disabled) {
    border-color: var(--border-strong);
    background: var(--panel-3);
  }
  .ghost {
    background: transparent;
    color: var(--muted);
  }
  .ghost:hover:not(:disabled) {
    background: var(--panel-2);
    color: var(--text);
  }
  .danger {
    background: var(--error-soft);
    border-color: rgba(239, 68, 68, 0.4);
    color: #fca5a5;
  }
  .danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.24);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
