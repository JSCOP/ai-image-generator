<script lang="ts">
  // Reference images: drop zone (files dropped anywhere on the window arrive through the screen), file picker button,
  // thumbnails with remove buttons. Disabled with a reason when the model takes no references.
  import { imageSrc } from '../lib/api';
  import { formatBytes } from '../lib/format';
  import type { ReferenceView } from '../lib/types';
  import Icon from './Icon.svelte';

  interface Props {
    references: ReferenceView[];
    max: number;
    disabled?: boolean;
    /** Why the picker is off. */
    reason?: string;
    /** Files are being dragged over the window. */
    dragging?: boolean;
    onpick: () => void;
    onremove: (path: string) => void;
    /** Opens the editor on a reference (mask edit, background removal). */
    onedit?: (path: string) => void;
  }

  let { references, max, disabled = false, reason = '', dragging = false, onpick, onremove, onedit }: Props = $props();

  const full = $derived(references.length >= max);
  const over = $derived(references.length > max);
</script>

<div class="refs" class:disabled class:dragging={dragging && !disabled}>
  <div class="head">
    <span class="label" id="ref-label">참조 이미지</span>
    <span class="count" class:over>{references.length}/{max}</span>
  </div>

  {#if disabled}
    <p class="reason"><Icon name="info" size={16} /> {reason}</p>
  {:else}
    <button type="button" class="drop" disabled={full} onclick={onpick} aria-describedby="ref-label">
      <Icon name="upload" size={18} />
      <span>{dragging ? '여기에 놓으세요' : full ? `최대 ${max}장까지 넣었습니다` : '끌어 놓기 · 눌러서 고르기 · Ctrl+V 붙여넣기'}</span>
      <small>PNG, JPEG, WebP · 장당 10MB · 한 변 8192px 이하</small>
    </button>
  {/if}

  {#if references.length > 0}
    <ul class="thumbs" class:dim={disabled}>
      {#each references as ref (ref.path)}
        <li title="{ref.path}&#10;{ref.width}×{ref.height} · {formatBytes(ref.bytes)}">
          <img src={imageSrc(ref.path)} alt={ref.name} loading="lazy" />
          <button type="button" class="x" aria-label="{ref.name} 빼기" onclick={() => onremove(ref.path)}>
            <Icon name="close" size={14} />
          </button>
          {#if onedit}
            <button type="button" class="x ed" aria-label="{ref.name} 편집" title="편집 · 배경 지우기" onclick={() => onedit(ref.path)}>
              <Icon name="edit" size={14} />
            </button>
          {/if}
        </li>
      {/each}
    </ul>
    {#if over}<p class="reason warn">이 모델은 {max}장까지 받습니다. 남는 이미지를 빼 주세요.</p>{/if}
  {/if}
</div>

<style>
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .count {
    font-size: 12px;
    color: var(--muted);
  }
  .count.over {
    color: var(--warn);
  }
  .drop {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: var(--s3) var(--s3);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--text);
    cursor: pointer;
  }
  .drop:hover:not(:disabled) {
    border-color: var(--accent-line);
    background: var(--accent-soft);
  }
  .drop:disabled {
    cursor: default;
    color: var(--muted);
  }
  .drop span {
    font-size: 13px;
  }
  .drop small {
    font-size: 12px;
    color: var(--faint);
  }
  .dragging .drop {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .thumbs {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--s2);
    margin: var(--s2) 0 0;
    padding: 0;
    list-style: none;
  }
  .thumbs.dim {
    opacity: 0.5;
  }
  li {
    position: relative;
    aspect-ratio: 1;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    overflow: hidden;
    background: var(--panel-2);
  }
  li img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .x {
    position: absolute;
    top: 3px;
    right: 3px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: rgba(12, 13, 16, 0.8);
    color: var(--text);
    cursor: pointer;
  }
  .x:hover {
    background: var(--error);
  }
  .x.ed {
    right: auto;
    left: 3px;
  }
  .x.ed:hover {
    background: var(--accent);
  }
  .reason {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin: 0;
    padding: var(--s2) var(--s3);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--muted);
    font-size: 12px;
  }
  .reason.warn {
    margin-top: var(--s2);
    background: var(--warn-soft);
    color: #fcd34d;
  }
</style>
