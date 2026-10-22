<script lang="ts">
  // One result card: waiting skeleton, running spinner, retry reason, failure reason, missing file, or the image
  // (transparent images on a checkerboard). Finished images open the viewer.
  import { imageSrc } from '../lib/api';
  import { formatElapsed } from '../lib/format';
  import type { DisplayCard } from '../lib/generateForm';
  import Icon from './Icon.svelte';

  interface Props {
    card: DisplayCard;
    /** Width / height of the requested size. */
    aspect?: number;
    onopen?: () => void;
  }

  let { card, aspect = 1, onopen }: Props = $props();

  let broken = $state(false);
  $effect(() => {
    // A new image path gets a new chance to load.
    void card.imagePath;
    broken = false;
  });

  // Very wide or tall sizes still give a usable card.
  const shape = $derived(Math.min(2.2, Math.max(0.55, aspect)));
  const STATUS_TEXT = {
    waiting: '대기 중',
    running: '생성 중',
    retrying: '재시도 대기',
    done: '완료',
    skipped: '이미 있음',
    failed: '실패',
    cancelled: '취소됨',
    missing: '파일 없음',
    pending: '끝나지 않음',
  } as const;
</script>

{#if card.imagePath && !broken}
  <button
    type="button"
    class="card image"
    class:checker={card.transparent}
    style:aspect-ratio={shape}
    title="크게 보기"
    aria-label="{card.label} 크게 보기"
    onclick={onopen}
  >
    <img src={imageSrc(card.imagePath)} alt="결과 {card.label}" loading="lazy" decoding="async" onerror={() => (broken = true)} />
    {#if card.label}<span class="tag">{card.label}</span>{/if}
    {#if card.status === 'skipped'}<span class="corner">이미 있던 이미지</span>
    {:else if card.elapsedSec !== undefined}<span class="corner">{formatElapsed(card.elapsedSec)}</span>{/if}
  </button>
{:else}
  <div class="card state {card.status}" style:aspect-ratio={shape} role="group" aria-label="{card.label} {STATUS_TEXT[card.status]}">
    {#if card.label}<span class="tag">{card.label}</span>{/if}
    <div class="middle">
      {#if card.status === 'running' || card.status === 'retrying'}
        <span class="spinner" class:warn={card.status === 'retrying'} aria-hidden="true"></span>
      {:else if card.status === 'failed'}
        <span class="ic err"><Icon name="alert" size={22} /></span>
      {:else if card.status === 'cancelled'}
        <span class="ic"><Icon name="stop" size={22} /></span>
      {:else if card.status === 'missing' || broken}
        <span class="ic"><Icon name="image" size={22} /></span>
      {:else if card.status === 'pending'}
        <span class="ic"><Icon name="history" size={22} /></span>
      {/if}
      <strong>{broken ? '이미지를 불러오지 못함' : STATUS_TEXT[card.status]}</strong>
      {#if card.text}<p title={card.text}>{card.text}</p>{/if}
      {#if card.detail}
        <details class="more">
          <summary>자세히</summary>
          <p class="raw">{card.detail}</p>
        </details>
      {/if}
    </div>
  </div>
{/if}

<style>
  .card {
    position: relative;
    width: 100%;
    min-height: 120px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    overflow: hidden;
  }
  .image:not(.checker) {
    background: var(--panel);
  }
  .image {
    padding: 0;
    cursor: zoom-in;
    display: block;
  }
  .image:hover {
    border-color: var(--accent-line);
  }
  .image:focus-visible {
    border-color: var(--accent);
  }
  img {
    width: 100%;
    height: 100%;
    /* The whole image, never cropped: a result of another shape shows with margins. */
    object-fit: contain;
    display: block;
  }
  .tag,
  .corner {
    position: absolute;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    background: rgba(12, 13, 16, 0.72);
    color: var(--text);
  }
  .tag {
    left: var(--s2);
    top: var(--s2);
  }
  .corner {
    right: var(--s2);
    bottom: var(--s2);
    color: var(--muted);
  }
  .state {
    background: var(--panel);
    display: grid;
    place-items: center;
  }
  .middle {
    max-width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s2);
    padding: var(--s5) var(--s4) var(--s4);
    text-align: center;
  }
  strong {
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }
  p {
    margin: 0;
    max-width: 100%;
    font-size: 12px;
    color: var(--muted);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .waiting {
    background: linear-gradient(100deg, var(--panel) 30%, var(--panel-2) 50%, var(--panel) 70%);
    background-size: 300% 100%;
    animation: shimmer 2.4s linear infinite;
  }
  .running {
    border-color: var(--accent-line);
  }
  .retrying {
    border-color: rgba(245, 158, 11, 0.5);
  }
  .retrying p {
    color: #fcd34d;
  }
  .failed {
    border-color: rgba(239, 68, 68, 0.45);
    background: linear-gradient(180deg, rgba(239, 68, 68, 0.08), var(--panel));
  }
  .failed strong,
  .err {
    color: #fca5a5;
  }
  .failed p {
    color: #fecaca;
  }
  .cancelled {
    border-color: var(--border);
    background: var(--panel);
  }
  .cancelled strong {
    color: var(--muted);
  }
  .more {
    max-width: 100%;
    font-size: 12px;
    color: var(--muted);
  }
  .more summary {
    cursor: pointer;
    border-radius: 4px;
  }
  .more summary:hover {
    color: var(--text);
  }
  .failed .more .raw {
    margin-top: var(--s1);
    font-family: var(--mono);
    font-size: 11px;
    color: var(--muted);
    -webkit-line-clamp: 6;
    line-clamp: 6;
  }
  .missing,
  .pending {
    border-style: dashed;
  }
  .ic {
    color: var(--faint);
  }
  .spinner {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 3px solid var(--accent-soft);
    border-top-color: var(--accent);
    animation: spin 1.8s linear infinite;
  }
  .spinner.warn {
    border-color: var(--warn-soft);
    border-top-color: var(--warn);
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes shimmer {
    from {
      background-position: 100% 0;
    }
    to {
      background-position: 0 0;
    }
  }
</style>
