<script lang="ts">
  // Image editor over the whole window: paint the area to change (or draw on the image), describe the change and run
  // it as a mask edit; or make the background transparent, on this PC (pixels kept) or with an AI model (redrawn).
  // The painted layers are separate canvases the size of the source; the source itself is never drawn on.
  import { imageSrc } from '../lib/api';
  import { addPoint, endSegment, newestSegment, paintSegments, strokeSegments, type Pt, type Stroke, type Tool } from '../lib/brush';
  import { downloadText, EDIT_COUNTS } from '../lib/editForm';
  import { closeEditor, cutOutLocally, editModel, editor, startAiTransparent, startMaskEdit, transparentModel } from '../lib/editor.svelte';
  import { gen } from '../lib/generate.svelte';
  import type { ReferenceView } from '../lib/types';
  import Button from './Button.svelte';
  import Segmented from './Segmented.svelte';
  import Select from './Select.svelte';

  interface Props {
    source: ReferenceView;
  }

  let { source }: Props = $props();

  let tool = $state<Tool>('mask');
  /** Brush diameter in screen pixels. */
  let brush = $state(40);
  let color = $state('#ff3b30');
  let prompt = $state('');
  let count = $state('1');
  let model = $state('');
  let aiModel = $state('');
  /** Finished strokes (undo removes the last) and undone ones (redo puts them back). */
  let strokes = $state.raw<Stroke[]>([]);
  let undone = $state.raw<Stroke[]>([]);
  const painted = $derived(strokes.some((s) => s.tool === 'mask'));
  const drawn = $derived(strokes.some((s) => s.tool === 'draw'));

  let stage: HTMLDivElement | undefined = $state();
  let maskCanvas: HTMLCanvasElement | undefined = $state();
  let drawCanvas: HTMLCanvasElement | undefined = $state();
  let box: HTMLDivElement | undefined = $state();
  /** Displayed size of the image (fitted into the stage). */
  let shown = $state({ width: 0, height: 0 });
  let cursor = $state<{ x: number; y: number } | null>(null);

  const editModels = $derived(gen.models.filter((m) => m.references).map((m) => ({ value: m.id, label: m.id })));
  const aiModels = $derived(gen.models.filter((m) => m.references && m.transparency).map((m) => ({ value: m.id, label: m.id })));
  const counts = EDIT_COUNTS.map((n) => ({ value: String(n), label: `${n}장` }));
  const tools: { value: Tool; label: string }[] = [
    { value: 'mask', label: '영역 칠하기' },
    { value: 'draw', label: '그리기' },
    { value: 'erase', label: '지우개' },
  ];
  const marked = $derived(painted || drawn);
  const canEdit = $derived(marked && !!prompt.trim() && !!model && !editor.busy);

  // A new source (e.g. the cut-out result) starts with empty layers.
  $effect(() => {
    void source.path;
    clearAll();
  });

  $effect(() => {
    if (!model || !gen.models.some((m) => m.id === model)) model = editModel();
    if (!aiModel || !gen.models.some((m) => m.id === aiModel)) aiModel = transparentModel();
  });

  $effect(() => {
    if (!stage) return;
    const fit = () => {
      const w = stage!.clientWidth;
      const h = stage!.clientHeight;
      const scale = Math.min(w / source.width, h / source.height);
      shown = { width: Math.max(1, Math.floor(source.width * scale)), height: Math.max(1, Math.floor(source.height * scale)) };
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(stage);
    return () => observer.disconnect();
  });

  $effect(() => {
    const before = document.activeElement;
    box?.focus();
    return () => (before as HTMLElement | null)?.focus?.();
  });

  function layersOf(stroke: Stroke): CanvasRenderingContext2D[] {
    const mask = maskCanvas?.getContext('2d');
    const draw = drawCanvas?.getContext('2d');
    if (!mask || !draw) return [];
    return stroke.tool === 'erase' ? [mask, draw] : stroke.tool === 'mask' ? [mask] : [draw];
  }

  /** Clears both layers and draws `list` again (undo, redo). */
  function replay(list: Stroke[]) {
    for (const canvas of [maskCanvas, drawCanvas]) canvas?.getContext('2d')?.clearRect(0, 0, source.width, source.height);
    for (const stroke of list) for (const ctx of layersOf(stroke)) paintSegments(ctx, stroke, strokeSegments(stroke.points));
  }

  function clearAll() {
    strokes = [];
    undone = [];
    replay([]);
  }

  function undo() {
    if (current || strokes.length === 0) return;
    undone = [...undone, strokes[strokes.length - 1]];
    strokes = strokes.slice(0, -1);
    replay(strokes);
  }

  function redo() {
    if (current || undone.length === 0) return;
    strokes = [...strokes, undone[undone.length - 1]];
    undone = undone.slice(0, -1);
    replay(strokes);
  }

  /** The stroke being drawn (not in `strokes` until the pointer is released). */
  let current: Stroke | null = null;

  /** Pointer position in source pixels. */
  function at(e: PointerEvent, target: HTMLElement): Pt {
    const r = target.getBoundingClientRect();
    return { x: ((e.clientX - r.left) * source.width) / r.width, y: ((e.clientY - r.top) * source.height) / r.height };
  }

  function extend(stroke: Stroke, p: Pt) {
    if (!addPoint(stroke.points, p)) return;
    const seg = newestSegment(stroke.points);
    if (seg) for (const ctx of layersOf(stroke)) paintSegments(ctx, stroke, [seg]);
  }

  function down(e: PointerEvent) {
    if (e.button !== 0 || editor.busy) return;
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    // The brush keeps its on-screen size: its width in source pixels follows the zoom.
    current = { tool, color, width: (brush * source.width) / Math.max(shown.width, 1), points: [] };
    extend(current, at(e, target));
  }

  function move(e: PointerEvent) {
    const target = e.currentTarget as HTMLElement;
    const r = target.getBoundingClientRect();
    cursor = { x: e.clientX - r.left, y: e.clientY - r.top };
    if (!current) return;
    // Points the browser merged between two events make fast strokes follow the hand.
    const events = typeof e.getCoalescedEvents === 'function' ? e.getCoalescedEvents() : [];
    for (const ev of events.length > 0 ? events : [e]) extend(current, at(ev, target));
  }

  function up() {
    const stroke = current;
    current = null;
    if (!stroke) return;
    const end = endSegment(stroke.points);
    if (end) for (const ctx of layersOf(stroke)) paintSegments(ctx, stroke, [end]);
    strokes = [...strokes, stroke];
    undone = [];
  }

  function submitEdit() {
    if (!canEdit || !maskCanvas || !drawCanvas) return;
    startMaskEdit(prompt, model, Number(count), maskCanvas.toDataURL('image/png'), drawn ? drawCanvas.toDataURL('image/png') : null);
  }

  function onkeydown(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLInputElement;
    if (e.key === 'Escape') {
      e.stopPropagation();
      closeEditor();
    } else if (e.key === 'Enter' && e.ctrlKey) submitEdit();
    else if (typing) return;
    else if (e.ctrlKey && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
    } else if (e.ctrlKey && e.key.toLowerCase() === 'y') {
      e.preventDefault();
      redo();
    } else if (e.key === '[') brush = Math.max(4, Math.round(brush / 1.25));
    else if (e.key === ']') brush = Math.min(200, Math.round(brush * 1.25));
  }
</script>

<div class="editor" role="dialog" aria-modal="true" aria-label="이미지 편집" tabindex="-1" bind:this={box} {onkeydown}>
  <div class="tools" role="toolbar" aria-label="편집 도구">
    <Segmented options={tools} bind:value={tool} label="도구" />
    <label class="brush">
      <span>붓 크기 {brush}px</span>
      <input type="range" min="4" max="200" bind:value={brush} aria-label="붓 크기" />
    </label>
    {#if tool === 'draw'}
      <label class="color"><span>색</span><input type="color" bind:value={color} /></label>
    {/if}
    <div class="history">
      <Button size="sm" variant="ghost" icon="undo" label="되돌리기" title="되돌리기 (Ctrl+Z)" disabled={strokes.length === 0} onclick={undo} />
      <Button size="sm" variant="ghost" icon="redo" label="다시 하기" title="다시 하기 (Ctrl+Shift+Z)" disabled={undone.length === 0} onclick={redo} />
    </div>
    <Button size="sm" variant="ghost" icon="trash" disabled={strokes.length === 0} onclick={clearAll}>모두 지우기</Button>
    <span class="keys">[ ] 붓 크기 · Ctrl+Z 되돌리기</span>
  </div>

  <div class="stage" bind:this={stage}>
    <div class="canvas checker" style:width="{shown.width}px" style:height="{shown.height}px">
      <img src={imageSrc(source.path)} alt={source.name} draggable="false" />
      <canvas class="mask" bind:this={maskCanvas} width={source.width} height={source.height}></canvas>
      <canvas bind:this={drawCanvas} width={source.width} height={source.height}></canvas>
      <div
        class="paint"
        role="presentation"
        onpointerdown={down}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={up}
        onpointerleave={() => (cursor = null)}
      ></div>
      {#if cursor}
        <span class="cursor" style:left="{cursor.x}px" style:top="{cursor.y}px" style:width="{brush}px" style:height="{brush}px"></span>
      {/if}
    </div>
  </div>

  <aside class="panel">
    <header>
      <h2 class="ellipsis" title={source.path}>{source.name}</h2>
      <span class="size">{source.width}×{source.height}</span>
      <Button variant="ghost" size="sm" icon="close" label="닫기" disabled={editor.busy} onclick={closeEditor} />
    </header>

    <section>
      <h3>칠한 부분 바꾸기</h3>
      <p class="hint">바꿀 곳을 <b>영역 칠하기</b>로 칠하거나 <b>그리기</b>로 대략 그려 주세요. 칠한 곳 밖은 원본 그대로 남습니다.</p>
      <textarea bind:value={prompt} rows="4" placeholder="예: 빨간 모자를 씌워 줘 / 이 그림을 나무 의자로 바꿔 줘" aria-label="바꿀 내용"></textarea>
      <div class="row">
        <Select options={editModels} bind:value={model} label="수정 모델" disabled={editModels.length === 0} />
        <Segmented options={counts} bind:value={count} label="장수" />
      </div>
      <Button variant="primary" icon="edit" full disabled={!canEdit} onclick={submitEdit}>칠한 부분 수정</Button>
      {#if !marked}<p class="note">먼저 이미지에 바꿀 곳을 칠해 주세요.</p>{:else if editModels.length === 0}<p class="note">이 연결에는 이미지를 받는 모델이 없습니다.</p>{/if}
    </section>

    <section>
      <h3>배경 투명하게</h3>
      <Button icon="scissors" full disabled={editor.busy} onclick={cutOutLocally}>
        {editor.busy ? (editor.download ? '모델 내려받는 중…' : '배경 지우는 중…') : '내 PC에서 배경 지우기'}
      </Button>
      {#if editor.download}
        <div class="bar" role="progressbar" aria-label="모델 내려받기" aria-valuemin={0} aria-valuemax={editor.download.total} aria-valuenow={editor.download.done}>
          <span style:width="{(editor.download.done / editor.download.total) * 100}%"></span>
        </div>
        <p class="note">{downloadText(editor.download.done, editor.download.total)} · 처음 한 번만 받습니다</p>
      {:else}
        <p class="note">원본 픽셀을 그대로 두고 배경만 지웁니다. 무료, 인터넷 없이 동작(처음 한 번 모델 170MB를 받습니다).</p>
      {/if}
      <div class="row">
        <Select options={aiModels} bind:value={aiModel} label="투명 배경 모델" disabled={aiModels.length === 0} />
      </div>
      <Button icon="generate" full disabled={!aiModel || editor.busy} onclick={() => startAiTransparent(aiModel)}>AI로 투명 배경 다시 그리기</Button>
      <p class="note">
        {aiModels.length === 0 ? '이 연결에는 투명 배경을 만드는 모델이 없습니다.' : '모델이 이미지를 새로 그리므로 모양이 조금 바뀔 수 있습니다. 생성 비용이 듭니다.'}
      </p>
    </section>
  </aside>
</div>

<style>
  .editor {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    grid-template-rows: auto minmax(0, 1fr);
    background: rgba(10, 11, 14, 0.97);
  }
  .editor:focus-visible {
    outline: none;
  }
  .tools {
    grid-column: 1;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--s3);
    padding: var(--s3) var(--s4);
    border-bottom: 1px solid var(--border);
  }
  .brush,
  .color {
    display: flex;
    align-items: center;
    gap: var(--s2);
    font-size: 12px;
    color: var(--muted);
  }
  .brush span {
    min-width: 92px;
  }
  .history {
    display: flex;
    gap: 2px;
  }
  .keys {
    margin-left: auto;
    font-size: 12px;
    color: var(--faint);
  }
  .color input {
    width: 32px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--border);
    background: none;
  }
  .stage {
    grid-column: 1;
    min-width: 0;
    min-height: 0;
    display: grid;
    place-items: center;
    margin: var(--s4);
    overflow: hidden;
  }
  .canvas {
    position: relative;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .canvas img,
  .canvas canvas,
  .paint {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .canvas img {
    object-fit: fill;
    user-select: none;
  }
  canvas.mask {
    opacity: 0.45;
  }
  .paint {
    cursor: none;
    touch-action: none;
  }
  .cursor {
    position: absolute;
    transform: translate(-50%, -50%);
    border: 1.5px solid #fff;
    border-radius: 50%;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.6);
    pointer-events: none;
  }
  .panel {
    grid-column: 2;
    grid-row: 1 / span 2;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border);
    background: var(--panel);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: var(--s3) var(--s3) var(--s3) var(--s4);
    border-bottom: 1px solid var(--border);
  }
  h2 {
    flex: 1;
    min-width: 0;
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .size {
    font-size: 12px;
    color: var(--muted);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    padding: var(--s4);
    border-bottom: 1px solid var(--border);
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .hint,
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--faint);
  }
  .hint b {
    color: var(--muted);
    font-weight: 600;
  }
  textarea {
    width: 100%;
    resize: vertical;
    padding: var(--s2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--panel-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
</style>
