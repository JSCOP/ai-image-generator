<script lang="ts">
  import Icon from './Icon.svelte';

  interface Props {
    value: number;
    min: number;
    max: number;
    step?: number;
    id?: string;
    label?: string;
    disabled?: boolean;
    onchange?: (value: number) => void;
  }

  let { value = $bindable(), min, max, step = 1, id, label, disabled = false, onchange }: Props = $props();

  // What the user types; committed (clamped) on blur or Enter.
  let draft = $state('');
  $effect(() => {
    draft = String(value);
  });

  function set(v: number) {
    const next = Math.min(max, Math.max(min, Math.round(v)));
    draft = String(next);
    if (next !== value) {
      value = next;
      onchange?.(next);
    }
  }

  function commit() {
    const n = Number(draft.trim());
    set(Number.isFinite(n) && draft.trim() !== '' ? n : value);
  }
</script>

<div class="stepper" class:disabled>
  <input
    {id}
    class="num"
    inputmode="numeric"
    aria-label={label}
    aria-valuemin={min}
    aria-valuemax={max}
    {disabled}
    bind:value={draft}
    onblur={commit}
    onkeydown={(e) => {
      if (e.key === 'Enter') commit();
      else if (e.key === 'ArrowUp') (e.preventDefault(), set(value + step));
      else if (e.key === 'ArrowDown') (e.preventDefault(), set(value - step));
    }}
  />
  <button type="button" aria-label="줄이기" disabled={disabled || value <= min} onclick={() => set(value - step)}>
    <Icon name="minus" size={16} />
  </button>
  <button type="button" aria-label="늘리기" disabled={disabled || value >= max} onclick={() => set(value + step)}>
    <Icon name="plus" size={16} />
  </button>
</div>

<style>
  .stepper {
    display: flex;
    height: 40px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .stepper:focus-within {
    border-color: var(--accent);
  }
  .num {
    flex: 1;
    min-width: 0;
    padding: 0 var(--s3);
    border: 0;
    background: transparent;
  }
  .num:focus-visible {
    outline: none;
  }
  .stepper:has(.num:focus-visible) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  button {
    width: 44px;
    border: 0;
    border-left: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--panel-3);
    color: var(--text);
  }
  button:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .disabled {
    opacity: 0.5;
  }
</style>
