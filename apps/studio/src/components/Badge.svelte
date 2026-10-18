<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    tone?: 'neutral' | 'accent' | 'ok' | 'warn' | 'error';
    title?: string;
    children: Snippet;
  }

  let { tone = 'neutral', title, children }: Props = $props();
</script>

<span class="badge {tone}" {title}>{@render children()}</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--s1);
    height: 22px;
    padding: 0 var(--s2);
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    background: var(--panel-3);
    color: var(--muted);
  }
  .accent {
    background: var(--accent-soft);
    color: #c4b5fd;
  }
  .ok {
    background: var(--ok-soft);
    color: #86efac;
  }
  .warn {
    background: var(--warn-soft);
    color: #fcd34d;
  }
  .error {
    background: var(--error-soft);
    color: #fca5a5;
  }
</style>
