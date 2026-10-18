<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon, { type IconName } from './Icon.svelte';

  interface Props {
    icon?: IconName;
    title: string;
    text?: string;
    /** Buttons under the text. */
    children?: Snippet;
  }

  let { icon = 'image', title, text, children }: Props = $props();
</script>

<div class="empty">
  <span class="ic"><Icon name={icon} size={28} /></span>
  <h3>{title}</h3>
  {#if text}<p>{text}</p>{/if}
  {#if children}<div class="actions">{@render children()}</div>{/if}
</div>

<style>
  .empty {
    height: 100%;
    min-height: 240px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--s2);
    padding: var(--s6);
    text-align: center;
  }
  .ic {
    width: 56px;
    height: 56px;
    display: grid;
    place-items: center;
    margin-bottom: var(--s2);
    border-radius: 14px;
    background: var(--accent-soft);
    color: #c4b5fd;
  }
  h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  p {
    margin: 0;
    max-width: 420px;
    color: var(--muted);
  }
  .actions {
    display: flex;
    gap: var(--s2);
    margin-top: var(--s3);
  }
</style>
