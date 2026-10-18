<script lang="ts" module>
  import type { GalleryItem } from '../lib/types';

  export interface ViewerEntry {
    key: string;
    imagePath: string;
    label: string;
    transparent: boolean;
    /** The gallery record of this image, when it is known yet. */
    item: GalleryItem | null;
    /** What the screen knows without a record (the run's input). */
    prompt: string;
    model: string;
    size: string;
  }
</script>

<script lang="ts">
  // Large view of one result: fit or 100 %, previous/next, and what the record says about it (model and request id,
  // provider size -> saved size, transparency). Buttons: show in Explorer, reuse settings, resume, use as reference.
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { imageSrc, toServiceError } from '../lib/api';
  import { formatRelativeTime, formatSize, truncateMiddle } from '../lib/format';
  import { splitPrompt } from '../lib/generateForm';
  import { toastError } from '../lib/state.svelte';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    entries: ViewerEntry[];
    index: number;
    onclose: () => void;
    onreuse?: (item: GalleryItem) => void;
    onresume?: (item: GalleryItem) => void;
    onreference?: (path: string) => void;
  }

  let { entries, index = $bindable(), onclose, onreuse, onresume, onreference }: Props = $props();

  let actual = $state(false);
  let box: HTMLDivElement | undefined = $state();
  const entry = $derived(entries[Math.min(Math.max(index, 0), entries.length - 1)]);
  const item = $derived(entry?.item ?? null);
  const prompt = $derived(splitPrompt(item?.prompt ?? entry?.prompt ?? ''));

  $effect(() => {
    const before = document.activeElement;
    box?.focus();
    return () => (before as HTMLElement | null)?.focus?.();
  });

  function go(step: number) {
    if (entries.length < 2) return;
    index = (index + step + entries.length) % entries.length;
    actual = false;
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    } else if (e.key === 'ArrowLeft') go(-1);
    else if (e.key === 'ArrowRight') go(1);
  }

  async function reveal() {
    if (!entry) return;
    try {
      await revealItemInDir(entry.imagePath);
    } catch (e) {
      toastError(toServiceError(e));
    }
  }

  const sizeLine = $derived.by(() => {
    if (item?.providerSize && item.savedSize) {
      return item.providerSize === item.savedSize
        ? formatSize(item.savedSize)
        : `${formatSize(item.providerSize)} → ${formatSize(item.savedSize)}`;
    }
    return formatSize(item?.savedSize ?? item?.size ?? entry?.size ?? '');
  });
</script>

{#if entry}
  <div class="viewer" role="dialog" aria-modal="true" aria-label="결과 크게 보기" tabindex="-1" bind:this={box} {onkeydown}>
    <div class="stage" class:actual>
      <button
        type="button"
        class="frame"
        class:checker={entry.transparent}
        title={actual ? '화면에 맞추기' : '원래 크기로 보기'}
        onclick={() => (actual = !actual)}
      >
        <img src={imageSrc(entry.imagePath)} alt="결과 {entry.label}" />
      </button>
      {#if entries.length > 1}
        <button type="button" class="nav prev" aria-label="이전 이미지" onclick={() => go(-1)}><Icon name="chevronLeft" size={22} /></button>
        <button type="button" class="nav next" aria-label="다음 이미지" onclick={() => go(1)}><Icon name="chevronRight" size={22} /></button>
      {/if}
    </div>

    <aside class="info">
      <header>
        <h2>{entry.label || '결과'} <span class="pos">{index + 1} / {entries.length}</span></h2>
        <Button variant="ghost" size="sm" icon={actual ? 'shrink' : 'expand'} label={actual ? '화면에 맞추기' : '원래 크기'} onclick={() => (actual = !actual)} />
        <Button variant="ghost" size="sm" icon="close" label="닫기" onclick={onclose} />
      </header>

      <dl>
        <dt>프롬프트</dt>
        <dd class="prompt">{prompt.prompt}</dd>
        {#if prompt.avoid}
          <dt>Avoid</dt>
          <dd class="prompt">{prompt.avoid}</dd>
        {/if}
        <dt>모델</dt>
        <dd>
          {item?.imageModel ?? entry.model}
          {#if item?.responseModel && item.responseModel !== item.imageModel}<span class="sub">응답 모델 {item.responseModel}</span>{/if}
          {#if item?.requestId}<span class="sub mono" title="요청 ID">요청 ID {item.requestId}</span>{/if}
        </dd>
        <dt>크기</dt>
        <dd>
          {sizeLine}
          {#if item?.providerSize && item.savedSize && item.providerSize !== item.savedSize}<span class="sub">제공자 원본 → 저장 크기</span>{/if}
        </dd>
        {#if item?.quality}
          <dt>품질</dt>
          <dd>{({ low: '낮음', medium: '보통', high: '높음' } as Record<string, string>)[item.quality] ?? item.quality}</dd>
        {/if}
        {#if entry.transparent || item?.transparentRatio != null}
          <dt>배경</dt>
          <dd>
            투명
            {#if item?.transparentRatio != null}<span class="sub ok">완전 투명 {(item.transparentRatio * 100).toFixed(1)}%</span>{/if}
          </dd>
        {/if}
        {#if item}
          <dt>만든 때</dt>
          <dd>{formatRelativeTime(item.modified)}</dd>
        {/if}
        <dt>파일</dt>
        <dd class="mono path" title={entry.imagePath}>{truncateMiddle(entry.imagePath, 64)}</dd>
      </dl>

      <div class="actions">
        <Button icon="folder" full onclick={reveal}>탐색기에서 보기</Button>
        {#if item && onreuse}
          <Button icon="refresh" full onclick={() => item && onreuse(item)}>이 설정 다시 쓰기</Button>
        {/if}
        {#if item && onresume}
          <Button icon="play" full onclick={() => item && onresume(item)}>이어서 생성</Button>
        {/if}
        {#if onreference}
          <Button icon="image" full onclick={() => onreference(entry.imagePath)}>참조로 사용</Button>
        {/if}
        {#if !item}<p class="note">기록을 읽는 중이면 일부 정보가 잠시 뒤에 나타납니다.</p>{/if}
      </div>
    </aside>
  </div>
{/if}

<style>
  .viewer {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    background: rgba(10, 11, 14, 0.96);
  }
  .viewer:focus-visible {
    box-shadow: none;
  }
  .stage {
    position: relative;
    min-width: 0;
    min-height: 0;
    display: grid;
    place-items: center;
    padding: var(--s5);
    overflow: hidden;
  }
  .stage.actual {
    overflow: auto;
    place-items: start;
  }
  .frame {
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    cursor: zoom-in;
    max-width: 100%;
    max-height: 100%;
    display: block;
  }
  .frame:not(.checker) {
    background: #0d0e11;
  }
  .actual .frame {
    max-width: none;
    max-height: none;
    cursor: zoom-out;
    margin: auto;
  }
  .frame img {
    display: block;
    max-width: calc(100vw - 320px - 2 * var(--s5));
    max-height: calc(100vh - 2 * var(--s5));
    object-fit: contain;
  }
  .actual .frame img {
    max-width: none;
    max-height: none;
  }
  .nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 40px;
    height: 56px;
    display: grid;
    place-items: center;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: rgba(29, 32, 39, 0.85);
    color: var(--text);
    cursor: pointer;
  }
  .nav:hover {
    background: var(--panel-3);
  }
  .prev {
    left: var(--s3);
  }
  .next {
    right: var(--s3);
  }
  .info {
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border);
    background: var(--panel);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--s1);
    padding: var(--s3) var(--s3) var(--s3) var(--s4);
    border-bottom: 1px solid var(--border);
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .pos {
    margin-left: var(--s2);
    font-size: 12px;
    font-weight: 400;
    color: var(--muted);
  }
  dl {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0;
    padding: var(--s3) var(--s4);
  }
  dt {
    margin-top: var(--s3);
    font-size: 12px;
    color: var(--faint);
  }
  dt:first-child {
    margin-top: 0;
  }
  dd {
    margin: 2px 0 0;
    overflow-wrap: anywhere;
  }
  .prompt {
    white-space: pre-wrap;
    max-height: 180px;
    overflow: auto;
  }
  .sub {
    display: block;
    font-size: 12px;
    color: var(--muted);
  }
  .sub.ok {
    color: #86efac;
  }
  .mono {
    font-family: var(--mono);
    font-size: 12px;
  }
  .path {
    color: var(--muted);
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    padding: var(--s3) var(--s4) var(--s4);
    border-top: 1px solid var(--border);
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--faint);
  }
</style>
