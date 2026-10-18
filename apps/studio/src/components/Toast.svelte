<script lang="ts">
  // Toast stack (bottom centre of the work area, clear of the action buttons), fed by state.toast().
  import { dismissToast, toasts } from '../lib/state.svelte';
  import Icon from './Icon.svelte';

  const ICON = { info: 'info', ok: 'check', warn: 'alert', error: 'alert' } as const;
</script>

<div class="stack" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast {t.tone}" role={t.tone === 'error' ? 'alert' : 'status'}>
      <span class="ic"><Icon name={ICON[t.tone]} size={18} /></span>
      <p>{t.text}</p>
      <button type="button" aria-label="알림 닫기" onclick={() => dismissToast(t.id)}><Icon name="close" size={16} /></button>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    /* Bottom centre of the work area (right of the nav rail): clear of the generate button (left), the save bars
       and gallery actions (right), and the header buttons (top). */
    left: calc(var(--nav-w) + (100vw - var(--nav-w) - min(380px, calc(100vw - 48px))) / 2);
    bottom: var(--s5);
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    width: min(380px, calc(100vw - 48px));
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: flex-start;
    gap: var(--s3);
    padding: var(--s3) var(--s3) var(--s3) var(--s4);
    background: var(--panel-2);
    border: 1px solid var(--border-strong);
    border-left: 3px solid var(--accent);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    animation: rise 0.16s ease-out;
  }
  .ok {
    border-left-color: var(--ok);
  }
  .warn {
    border-left-color: var(--warn);
  }
  .error {
    border-left-color: var(--error);
  }
  .ic {
    margin-top: 1px;
    color: var(--accent);
  }
  .ok .ic {
    color: var(--ok);
  }
  .warn .ic {
    color: var(--warn);
  }
  .error .ic {
    color: var(--error);
  }
  p {
    flex: 1;
    margin: 0;
    overflow-wrap: anywhere;
  }
  button {
    border: 0;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 2px;
    border-radius: 4px;
  }
  button:hover {
    color: var(--text);
  }
  @keyframes rise {
    from {
      transform: translateY(6px);
      opacity: 0;
    }
  }
</style>
