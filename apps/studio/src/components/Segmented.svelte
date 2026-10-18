<script lang="ts" generics="T extends string">
  interface Segment<V extends string> {
    value: V;
    label: string;
    disabled?: boolean;
    /** Why it is disabled, shown on hover. */
    title?: string;
  }

  interface Props {
    options: Segment<T>[];
    value: T;
    label: string;
    disabled?: boolean;
    onchange?: (value: T) => void;
  }

  let { options, value = $bindable(), label, disabled = false, onchange }: Props = $props();

  function pick(v: T) {
    if (v === value) return;
    value = v;
    onchange?.(v);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
    const usable = options.filter((o) => !o.disabled);
    const at = usable.findIndex((o) => o.value === value);
    const step = e.key === 'ArrowRight' ? 1 : -1;
    const next = usable[(at + step + usable.length) % usable.length];
    if (next) {
      e.preventDefault();
      pick(next.value);
      const group = e.currentTarget as HTMLElement;
      group.querySelector<HTMLButtonElement>(`[data-value="${CSS.escape(next.value)}"]`)?.focus();
    }
  }
</script>

<div class="seg" role="radiogroup" aria-label={label} tabindex="-1" {onkeydown}>
  {#each options as opt (opt.value)}
    <button
      type="button"
      role="radio"
      aria-checked={opt.value === value}
      data-value={opt.value}
      class:on={opt.value === value}
      tabindex={opt.value === value ? 0 : -1}
      disabled={disabled || opt.disabled}
      title={opt.title}
      onclick={() => pick(opt.value)}
    >
      {opt.label}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: flex;
    padding: 3px;
    gap: 3px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  button {
    flex: 1;
    height: 34px;
    padding: 0 var(--s3);
    white-space: nowrap;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-weight: 600;
  }
  button:hover:not(:disabled):not(.on) {
    color: var(--text);
    background: var(--panel-3);
  }
  .on {
    background: var(--accent);
    color: #fff;
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
