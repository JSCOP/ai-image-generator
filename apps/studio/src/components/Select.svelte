<script lang="ts" generics="T extends string">
  import Icon from './Icon.svelte';

  interface Option<V extends string> {
    value: V;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    options: Option<T>[];
    value: T;
    id?: string;
    disabled?: boolean;
    /** Accessible name when no <label for> points here. */
    label?: string;
    onchange?: (value: T) => void;
  }

  let { options, value = $bindable(), id, disabled = false, label, onchange }: Props = $props();
</script>

<div class="select" class:disabled>
  <select
    {id}
    {disabled}
    aria-label={label}
    bind:value
    onchange={() => onchange?.(value)}
  >
    {#each options as opt (opt.value)}
      <option value={opt.value} disabled={opt.disabled}>{opt.label}</option>
    {/each}
  </select>
  <span class="chev"><Icon name="chevronDown" size={16} /></span>
</div>

<style>
  .select {
    position: relative;
  }
  select {
    appearance: none;
    width: 100%;
    height: 40px;
    padding: 0 36px 0 var(--s3);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    cursor: pointer;
    text-overflow: ellipsis;
  }
  select:hover {
    border-color: var(--border-strong);
  }
  select:focus-visible {
    border-color: var(--accent);
  }
  option {
    background: var(--panel);
  }
  .chev {
    position: absolute;
    right: var(--s3);
    top: 50%;
    transform: translateY(-50%);
    color: var(--muted);
    pointer-events: none;
  }
  .disabled select {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
