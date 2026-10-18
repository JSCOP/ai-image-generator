<script lang="ts">
  // One gallery record: thumbnail (checkerboard for transparent images) or the state of a record without an image
  // (failed with its cause, file missing, unfinished), status badge, prompt, model and size. Click selects, double click opens.
  import { imageSrc } from '../lib/api';
  import { formatSize, oneLine, truncateEnd } from '../lib/format';
  import { statusBadge } from '../lib/galleryView';
  import type { GalleryItem } from '../lib/types';
  import Badge from './Badge.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    item: GalleryItem;
    selected: boolean;
    onselect: () => void;
    onopen: () => void;
  }

  let { item, selected, onselect, onopen }: Props = $props();

  let broken = $state(false);
  $effect(() => {
    void item.imagePath;
    broken = false;
  });

  const badge = $derived(statusBadge(item));
  const hasImage = $derived(item.status === 'complete' || item.status === 'legacy');
  const transparent = $derived(item.background === 'transparent');
  const prompt = $derived(truncateEnd(oneLine(item.prompt), 90) || item.topic);
</script>

<button
  type="button"
  class="card {item.status}"
  class:selected
  aria-pressed={selected}
  title={oneLine(item.prompt)}
  onclick={onselect}
  ondblclick={() => hasImage && !broken && onopen()}
>
  <span class="thumb" class:checker={transparent && hasImage && !broken}>
    {#if hasImage && !broken}
      <img src={imageSrc(item.imagePath)} alt="" loading="lazy" decoding="async" class:contain={transparent} onerror={() => (broken = true)} />
    {:else}
      <span class="state">
        {#if item.status === 'failed'}
          <span class="ic err"><Icon name="alert" size={22} /></span>
          <span class="reason">{item.error || '실패'}</span>
        {:else if item.status === 'pending'}
          <span class="ic"><Icon name="history" size={22} /></span>
          <span class="reason">생성이 끝나기 전에 멈춘 기록</span>
        {:else}
          <span class="ic"><Icon name="image" size={22} /></span>
          <span class="reason">{broken ? '이미지를 불러오지 못했습니다' : '기록은 있지만 파일 없음'}</span>
        {/if}
      </span>
    {/if}
    <span class="badge"><Badge tone={badge.tone} title={badge.title}>{badge.label}</Badge></span>
  </span>
  <span class="text">
    <span class="prompt ellipsis">{prompt}</span>
    <span class="meta ellipsis">{item.imageModel} · {formatSize(item.savedSize ?? item.size)}</span>
  </span>
</button>

<style>
  .card {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--text);
    text-align: left;
    cursor: pointer;
    overflow: hidden;
  }
  .card:hover {
    border-color: var(--border-strong);
  }
  .card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .thumb {
    position: relative;
    display: block;
    aspect-ratio: 4 / 3;
    overflow: hidden;
  }
  .thumb:not(.checker) {
    background: var(--panel-2);
  }
  img {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: cover;
  }
  img.contain {
    object-fit: contain;
  }
  .badge {
    position: absolute;
    left: var(--s2);
    top: var(--s2);
    border-radius: 999px;
    background: rgba(12, 13, 16, 0.6);
  }
  .state {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--s2);
    padding: var(--s5) var(--s3) var(--s3);
    text-align: center;
  }
  .failed .thumb {
    background: linear-gradient(180deg, rgba(239, 68, 68, 0.1), var(--panel-2));
  }
  .missing .thumb,
  .pending .thumb {
    background:
      repeating-linear-gradient(135deg, transparent 0 10px, rgba(255, 255, 255, 0.02) 10px 20px),
      var(--panel-2);
  }
  .ic {
    color: var(--faint);
  }
  .err {
    color: #fca5a5;
  }
  .reason {
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
  .failed .reason {
    color: #fecaca;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--s2) var(--s3);
    min-width: 0;
  }
  .prompt {
    font-size: 13px;
  }
  .meta {
    font-size: 12px;
    color: var(--muted);
  }
</style>
