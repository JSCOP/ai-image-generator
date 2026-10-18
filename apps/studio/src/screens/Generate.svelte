<script lang="ts">
  // Generate screen: settings panel (left), live result cards (center), history by topic (right).
  // The run and the form live in lib/generate.svelte.ts, so leaving this screen neither stops nor forgets a run.
  import { untrack } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { open } from '@tauri-apps/plugin-dialog';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import Empty from '../components/Empty.svelte';
  import Icon from '../components/Icon.svelte';
  import ImageCard from '../components/ImageCard.svelte';
  import ReferencePicker from '../components/ReferencePicker.svelte';
  import Segmented from '../components/Segmented.svelte';
  import Select from '../components/Select.svelte';
  import Stepper from '../components/Stepper.svelte';
  import Viewer, { type ViewerEntry } from '../components/Viewer.svelte';
  import { errorText, imageSrc, toServiceError } from '../lib/api';
  import { formatRelativeTime, oneLine } from '../lib/format';
  import {
    gen,
    addReferencePaths,
    cancelRun,
    chooseConnection,
    fitToModel,
    initGenerate,
    loadHistory,
    loadModels,
    referenceLimit,
    removeReference,
    resumeGroup,
    retryFailed,
    reuseItem,
    startRun,
    stopResume,
    syncConnection,
  } from '../lib/generate.svelte';
  import {
    aspectOf,
    capabilities,
    connectionChoices,
    connectionState,
    CUSTOM_SIZE,
    historyGroups,
    itemCard,
    MAX_COUNT,
    parseSize,
    progressView,
    QUALITIES,
    resumeBlocked,
    runCard,
    runOutcome,
    runPills,
    samePath,
    SIZE_PRESETS,
    tallyCards,
    toInput,
    type DisplayCard,
    type HistoryGroup,
    type Quality,
  } from '../lib/generateForm';
  import { app, navigate, toast, toastError } from '../lib/state.svelte';
  import type { GalleryItem } from '../lib/types';

  initGenerate();
  $effect(() => {
    void app.connections;
    untrack(syncConnection);
  });

  const form = gen.form;
  const connState = $derived(connectionState(app.connections, app.connectionsError));
  const choices = $derived(connectionChoices(app.connections));
  const model = $derived(gen.models.find((m) => m.id === form.model));
  const caps = $derived(capabilities(model));
  const check = $derived(toInput(form, gen.models));
  const customCheck = $derived(form.size === CUSTOM_SIZE ? parseSize(form.customSize) : null);
  const groups = $derived(historyGroups(gen.history));
  const group = $derived(gen.view === 'run' ? undefined : groups.find((g) => g.key === gen.view));
  const groupResumeBlocked = $derived(group ? resumeBlocked(group.folder, gen.folders) : null);
  const run = $derived(gen.run);

  const cards: DisplayCard[] = $derived.by(() => {
    if (group) return group.items.map(itemCard);
    if (!run) return [];
    const transparent = run.input.background === 'transparent';
    return run.cards.map((c) => runCard(c, transparent, run.cancelling));
  });
  const tally = $derived(run && !group ? tallyCards(cards) : null);
  const finished = $derived(!!run && !gen.running && !!run.summary);
  const outcome = $derived(tally && finished && run?.summary ? runOutcome(tally, run.summary.elapsedSec, run.summary.cancelled) : null);
  const bar = $derived(tally ? progressView(tally, gen.running, !!run?.summary?.cancelled) : null);
  const retryLabel = $derived(tally && tally.failed > 0 ? '실패한 장 다시 만들기' : '남은 장 이어서 만들기');
  const canRetry = $derived(finished && !!tally && tally.failed + tally.cancelled > 0 && !!run?.topic);

  // History: a column on wide windows, a drawer behind a slim rail below 1366px (more room for the results).
  let historyOpen = $state(false);
  const aspect = $derived(aspectOf(group ? group.items[0]?.size : run?.input.size));

  // ---- viewer ----
  let viewerIndex: number | null = $state(null);
  const entries: ViewerEntry[] = $derived.by(() => {
    const out: ViewerEntry[] = [];
    if (group) {
      for (const it of group.items) {
        const card = itemCard(it);
        if (card.imagePath) out.push({ key: card.key, imagePath: card.imagePath, label: card.label, transparent: card.transparent, item: it, prompt: it.prompt, model: it.imageModel, size: it.size });
      }
    } else if (run) {
      for (const card of cards) {
        if (!card.imagePath) continue;
        const path = card.imagePath;
        const it = gen.history.find((i) => samePath(i.imagePath, path)) ?? null;
        out.push({ key: card.key, imagePath: path, label: card.label, transparent: card.transparent, item: it, prompt: run.input.prompt, model: run.input.imageModel, size: run.input.size });
      }
    }
    return out;
  });

  function openViewer(card: DisplayCard) {
    const at = entries.findIndex((e) => e.key === card.key);
    if (at >= 0) viewerIndex = at;
  }

  function groupOf(item: GalleryItem): HistoryGroup | undefined {
    return groups.find((g) => g.items.some((i) => i.recordPath === item.recordPath));
  }

  // ---- references: picker and window drop ----
  let dragging = $state(false);

  $effect(() => {
    let stop: (() => void) | undefined;
    let alive = true;
    try {
      getCurrentWebview()
        .onDragDropEvent((event) => {
          const p = event.payload;
          if (p.type === 'enter' || p.type === 'over') dragging = true;
          else if (p.type === 'leave') dragging = false;
          else if (p.type === 'drop') {
            dragging = false;
            if (model && !caps.references) toast(caps.referencesReason, 'warn');
            else void addReferencePaths(p.paths);
          }
        })
        .then(
          (unlisten) => (alive ? (stop = unlisten) : unlisten()),
          () => {},
        );
    } catch {
      // Outside the app window (plain browser) there is no drop event.
    }
    return () => {
      alive = false;
      stop?.();
    };
  });

  async function pickReferences() {
    try {
      const picked = await open({
        title: '참조 이미지 고르기',
        multiple: true,
        directory: false,
        filters: [{ name: '이미지', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }],
      });
      if (!picked) return;
      await addReferencePaths(Array.isArray(picked) ? picked : [picked]);
    } catch (e) {
      toastError(toServiceError(e));
    }
  }

  async function reveal(path: string) {
    try {
      await revealItemInDir(path);
    } catch (e) {
      toastError(toServiceError(e));
    }
  }

  // ---- form helpers ----
  let avoidOpen = $state(form.avoid.trim() !== '');
  const promptLength = $derived(Array.from(form.prompt).length);
  const qualities = QUALITIES.map((q) => ({ value: q.value, label: q.label }));
  const backgrounds = $derived([
    { value: 'auto' as const, label: '자동' },
    { value: 'transparent' as const, label: '투명(모델 자체)', disabled: !caps.transparency, title: caps.transparencyReason || undefined },
  ]);
  const modelOptions = $derived(gen.models.map((m) => ({ value: m.id, label: `${m.id} · ${m.provider}` })));

  function onPromptKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      if (check.ok && !gen.running) void startRun();
    }
  }

  const buttonText = $derived(form.resume ? `${form.count}장 이어서 생성` : `${form.count}장 생성`);
  const gridCols = $derived(aspect > 1.4 ? 'minmax(280px, 1fr)' : 'minmax(200px, 1fr)');
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape' && historyOpen && viewerIndex === null) historyOpen = false;
  }}
/>

<div class="gen" class:drawer-open={historyOpen}>
  <!-- ===== left: settings ===== -->
  <aside class="left" aria-label="생성 설정">
    <div class="scroll">
      <section>
        <div class="row-head">
          <label class="label" for="prompt">프롬프트</label>
          <span class="hint">{promptLength}자</span>
        </div>
        <textarea
          id="prompt"
          class="field prompt"
          rows="5"
          placeholder="만들 그림을 설명하세요. 예: 아늑한 독서 공간, 오후 햇살"
          bind:value={form.prompt}
          onkeydown={onPromptKey}
        ></textarea>
        <button type="button" class="fold" aria-expanded={avoidOpen} onclick={() => (avoidOpen = !avoidOpen)}>
          <Icon name={avoidOpen ? 'chevronDown' : 'chevronRight'} size={16} />
          <span>피할 것(Avoid)</span>
          {#if !avoidOpen && form.avoid.trim()}<span class="hint">입력됨</span>{/if}
        </button>
        {#if avoidOpen}
          <textarea class="field avoid" rows="2" aria-label="피할 것" placeholder="예: 글자, 워터마크, 흐린 초점" bind:value={form.avoid}></textarea>
        {/if}
      </section>

      <section>
        <label class="label" for="conn">연결</label>
        {#if choices.length > 0 && form.connection}
          <Select id="conn" options={choices} value={form.connection} disabled={gen.running} onchange={(v) => chooseConnection(v)} />
        {:else}
          <p class="box muted">{connState === 'loading' ? '연결을 불러오는 중…' : '쓸 수 있는 연결이 없습니다.'}</p>
        {/if}
      </section>

      <section>
        <label class="label" for="model">모델</label>
        {#if gen.modelsLoading}
          <p class="box muted"><span class="mini-spin" aria-hidden="true"></span> 모델 목록을 불러오는 중…</p>
        {:else if gen.modelsError}
          <div class="box error">
            <p>모델 목록을 받지 못했습니다. {errorText(gen.modelsError)}</p>
            <div class="box-actions">
              <Button size="sm" icon="refresh" onclick={loadModels}>다시 시도</Button>
              <Button size="sm" variant="ghost" onclick={() => navigate('connections')}>연결 확인하기</Button>
            </div>
          </div>
        {:else if form.connection && gen.models.length === 0}
          <div class="box muted">
            <p>이 연결에서 쓸 수 있는 이미지 모델이 없습니다.</p>
            <div class="box-actions"><Button size="sm" icon="refresh" onclick={loadModels}>다시 시도</Button></div>
          </div>
        {:else if gen.models.length > 0}
          <Select id="model" options={modelOptions} value={form.model} onchange={(v) => ((form.model = v), fitToModel())} />
          <div class="caps">
            {#if caps.references}<Badge tone="ok">참조 {caps.maxReferences}장까지</Badge>{:else}<Badge>참조 불가</Badge>{/if}
            {#if caps.transparency}<Badge tone="accent">투명 배경 가능</Badge>{:else}<Badge>투명 불가</Badge>{/if}
          </div>
        {:else}
          <p class="box muted">연결을 고르면 쓸 수 있는 모델이 나옵니다.</p>
        {/if}
      </section>

      <section>
        <span class="label" id="size-label">크기</span>
        <div class="chips" role="group" aria-labelledby="size-label">
          {#each SIZE_PRESETS as preset (preset.value)}
            <button type="button" class="chip" class:on={form.size === preset.value} aria-pressed={form.size === preset.value} onclick={() => (form.size = preset.value)}>
              <span>{preset.label}</span><small>{preset.ratio}</small>
            </button>
          {/each}
          <button type="button" class="chip" class:on={form.size === CUSTOM_SIZE} aria-pressed={form.size === CUSTOM_SIZE} onclick={() => (form.size = CUSTOM_SIZE)}>
            <span>직접 입력</span><small>가로x세로</small>
          </button>
        </div>
        {#if form.size === CUSTOM_SIZE}
          <input class="field custom" aria-label="직접 입력한 크기" placeholder="예: 1600x900" bind:value={form.customSize} />
          {#if customCheck && !customCheck.ok && form.customSize.trim()}<p class="err-text">{customCheck.message}</p>{/if}
        {/if}
      </section>

      <section>
        <span class="label">품질</span>
        <Segmented label="품질" options={qualities} value={form.quality} onchange={(v: Quality) => (form.quality = v)} />
      </section>

      <section>
        <label class="label" for="count">장수</label>
        <Stepper id="count" min={1} max={MAX_COUNT} value={form.count} onchange={(v) => (form.count = v)} />
      </section>

      <section>
        <ReferencePicker
          references={form.references}
          max={model ? caps.maxReferences : referenceLimit()}
          disabled={!!model && !caps.references}
          reason={caps.referencesReason}
          {dragging}
          onpick={pickReferences}
          onremove={removeReference}
        />
      </section>

      <section>
        <span class="label">배경</span>
        <Segmented
          label="배경"
          options={backgrounds}
          value={form.transparent ? 'transparent' : 'auto'}
          onchange={(v) => (form.transparent = v === 'transparent')}
        />
        {#if model && !caps.transparency}
          <p class="hint below">{caps.transparencyReason} 투명 배경은 gpt-image-2.5-sunburst / -flare처럼 지원하는 모델에서 고를 수 있습니다.</p>
        {:else if form.transparent}
          <p class="hint below">투명 PNG로 저장하고, 투명 픽셀 비율을 기록합니다.</p>
        {/if}
      </section>
    </div>

    <div class="action">
      {#if connState === 'none'}
        <p class="need"><Icon name="info" size={16} /> 연결을 먼저 추가하세요.</p>
        <Button variant="primary" size="lg" icon="connections" full onclick={() => navigate('connections')}>연결 화면으로</Button>
      {:else}
        {#if form.resume && form.topic}
          <div class="resume">
            <span class="ellipsis" title={form.topic}>이어서: {form.topic}</span>
            <Button variant="ghost" size="sm" icon="close" label="이어서 생성 해제" onclick={stopResume} />
          </div>
        {/if}
        {#if gen.running}
          <Button variant="danger" size="lg" icon="stop" full disabled={run?.cancelling} onclick={cancelRun}>
            {run?.cancelling ? '취소하는 중…' : '취소'}
          </Button>
        {:else}
          <Button variant="primary" size="lg" icon="generate" full disabled={!check.ok} onclick={startRun}>{buttonText}</Button>
          {#if !check.ok}<p class="why">{check.errors[0]}</p>{:else}<p class="why">Ctrl+Enter로도 시작합니다.</p>{/if}
        {/if}
      {/if}
    </div>
  </aside>

  <!-- ===== center: results ===== -->
  <section class="center" aria-label="결과">
    {#if connState === 'none' && !run && !group}
      <Empty icon="connections" title="연결을 먼저 추가하세요" text="이미지를 만들려면 CLIProxyAPI나 공식 API 연결이 하나 있어야 합니다.">
        <Button variant="primary" icon="plus" onclick={() => navigate('connections')}>연결 추가하기</Button>
      </Empty>
    {:else if connState === 'error' && !run && !group}
      <Empty icon="alert" title="연결 정보를 읽지 못했습니다" text={app.connectionsError ? errorText(app.connectionsError) : ''}>
        <Button icon="connections" onclick={() => navigate('connections')}>연결 화면에서 보기</Button>
      </Empty>
    {:else if group}
      <header class="result-head">
        <div class="title">
          <h2 class="ellipsis" title={group.prompt}>{oneLine(group.prompt) || group.topic}</h2>
          <p class="meta ellipsis">{group.topic} · {formatRelativeTime(group.modified)} · {group.model}</p>
        </div>
        <div class="counters">
          <span class="cnt ok">완료 {group.counts.done}</span>
          {#if group.counts.failed}<span class="cnt fail">실패 {group.counts.failed}</span>{/if}
          {#if group.counts.other}<span class="cnt">기타 {group.counts.other}</span>{/if}
        </div>
        <div class="head-actions">
          {#if run}<Button size="sm" variant="ghost" icon="chevronLeft" onclick={() => (gen.view = 'run')}>현재 실행</Button>{/if}
          <Button size="sm" icon="refresh" disabled={gen.running} onclick={() => reuseItem(group.items[0])}>이 설정 다시 쓰기</Button>
          <Button
            size="sm"
            icon="play"
            disabled={gen.running || !!groupResumeBlocked}
            title={groupResumeBlocked ?? undefined}
            onclick={() => resumeGroup(group.items)}>이어서 생성</Button
          >
          <Button size="sm" variant="ghost" icon="folder" label="탐색기에서 보기" title="탐색기에서 보기" onclick={() => reveal(group.items[0].imagePath)} />
        </div>
        {#if groupResumeBlocked}
          <p class="head-note"><Icon name="info" size={14} />{groupResumeBlocked}</p>
        {/if}
      </header>
      <div class="grid" style:--cols={gridCols}>
        {#each cards as card (card.key)}
          <ImageCard {card} {aspect} onopen={() => openViewer(card)} />
        {/each}
      </div>
    {:else if run}
      <header class="result-head">
        <div class="title">
          <h2 class="ellipsis" title={run.input.prompt}>{oneLine(run.input.prompt)}</h2>
          <p class="meta ellipsis" title={[run.input.imageModel, run.input.size.replace('x', '×'), run.topic].filter(Boolean).join(' · ')}>
            {[run.input.imageModel, run.input.size.replace('x', '×'), run.topic].filter(Boolean).join(' · ')}
          </p>
        </div>
        {#if outcome}
          <p class="outcome {outcome.tone}" aria-live="polite">{outcome.line}</p>
        {:else if tally}
          <div class="counters" aria-live="polite">
            {#each runPills(tally) as pill (pill.key)}<span class="cnt {pill.tone}">{pill.label}</span>{/each}
            {#if run.cancelling}<span class="cnt neutral">취소하는 중…</span>{/if}
          </div>
        {/if}
        <div class="head-actions">
          {#if canRetry}
            <Button size="sm" variant="primary" icon="refresh" onclick={retryFailed}>{retryLabel}</Button>
          {/if}
          {#if run.outputDir && finished}
            <Button size="sm" icon="folder" onclick={() => reveal(run.summary?.outputs[0] ?? run.outputDir)}>폴더 열기</Button>
          {:else if run.outputDir}
            <Button size="sm" variant="ghost" icon="folder" label="폴더 열기" title="폴더 열기" onclick={() => reveal(run.outputDir)} />
          {/if}
        </div>
      </header>
      {#if tally && bar && tally.total > 0}
        <div class="bar" role="progressbar" aria-label="진행" aria-valuemin={0} aria-valuemax={tally.total} aria-valuenow={tally.done + tally.failed + tally.cancelled}>
          <span style:width="{bar.fraction * 100}%" class={bar.tone}></span>
        </div>
      {/if}
      {#if cards.length > 0}
        <div class="grid" style:--cols={gridCols}>
          {#each cards as card (card.key)}
            <ImageCard {card} {aspect} onopen={() => openViewer(card)} />
          {/each}
        </div>
      {:else if run.error}
        <Empty icon="alert" title="생성을 시작하지 못했습니다" text={errorText(run.error)} />
      {:else}
        <div class="starting"><span class="mini-spin" aria-hidden="true"></span> 시작하는 중…</div>
      {/if}
    {:else}
      <Empty icon="generate" title="무엇을 만들까요?" text="왼쪽에 프롬프트를 쓰고 생성 버튼을 누르세요. 진행 상황이 여기에 장마다 나타납니다." />
    {/if}
  </section>

  <!-- ===== right: history (a drawer below 1366px) ===== -->
  <div class="rail">
    <button type="button" class="rail-btn" aria-expanded={historyOpen} aria-controls="gen-history" title="기록 열기" onclick={() => (historyOpen = !historyOpen)}>
      <Icon name="history" size={18} />
      <span>기록</span>
    </button>
  </div>
  {#if historyOpen}
    <button type="button" class="scrim" aria-label="기록 닫기" tabindex="-1" onclick={() => (historyOpen = false)}></button>
  {/if}
  <aside class="right" id="gen-history" aria-label="기록">
    <header class="hist-head">
      <h2>기록</h2>
      <div class="hist-tools">
        <Button size="sm" variant="ghost" icon="refresh" label="기록 새로 읽기" title="기록 새로 읽기" disabled={gen.historyLoading} onclick={loadHistory} />
        <span class="drawer-close"><Button size="sm" variant="ghost" icon="close" label="기록 닫기" title="기록 닫기" onclick={() => (historyOpen = false)} /></span>
      </div>
    </header>
    <div class="hist">
      {#if gen.historyError}
        <p class="box error">기록을 읽지 못했습니다. {errorText(gen.historyError)}</p>
      {:else if groups.length === 0}
        <p class="box muted">{gen.historyLoading ? '기록을 읽는 중…' : '아직 기록이 없습니다. 만든 이미지가 여기에 주제별로 쌓입니다.'}</p>
      {:else}
        <ul>
          {#each groups as g (g.key)}
            <li>
              <button type="button" class="entry" class:on={gen.view === g.key} aria-current={gen.view === g.key} onclick={() => ((gen.view = g.key), (historyOpen = false))}>
                <span class="thumbs" class:one={g.thumbs.length === 1}>
                  {#each g.thumbs as path (path)}
                    <img src={imageSrc(path)} alt="" loading="lazy" />
                  {:else}
                    <span class="no-thumb"><Icon name={g.counts.failed ? 'alert' : 'image'} size={18} /></span>
                  {/each}
                </span>
                <span class="text">
                  <span class="p" title={g.prompt}>{oneLine(g.prompt) || g.topic}</span>
                  <span class="m">{g.items.length}장 · {formatRelativeTime(g.modified)}</span>
                  <span class="m ellipsis">
                    {g.model}{#if g.counts.failed}<span class="f">{` · 실패 ${g.counts.failed}`}</span>{/if}
                  </span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </aside>
</div>

{#if viewerIndex !== null && entries.length > 0}
  <Viewer
    {entries}
    bind:index={viewerIndex}
    onclose={() => (viewerIndex = null)}
    onreuse={(item) => {
      viewerIndex = null;
      void reuseItem(item);
    }}
    onresume={(item) => {
      viewerIndex = null;
      const g = groupOf(item);
      void resumeGroup(g ? g.items : [item]);
    }}
    resumeNote={(item) => resumeBlocked(item.folder, gen.folders)}
    onreference={(path) => {
      viewerIndex = null;
      void addReferencePaths([path]);
    }}
  />
{/if}

<style>
  .gen {
    position: relative;
    height: 100%;
    display: grid;
    grid-template-columns: var(--panel-left-w) minmax(0, 1fr) var(--panel-right-w);
  }
  .rail,
  .scrim,
  .drawer-close {
    display: none;
  }
  /* Below 1366px the history becomes a drawer behind a slim rail, so the results get the width. */
  @media (max-width: 1365px) {
    .gen {
      grid-template-columns: 320px minmax(0, 1fr) 56px;
    }
    .rail {
      display: flex;
      justify-content: center;
      padding-top: var(--s3);
      border-left: 1px solid var(--border);
      background: var(--bg);
    }
    .rail-btn {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 2px;
      width: 44px;
      height: fit-content;
      padding: var(--s2) 0;
      border: 1px solid transparent;
      border-radius: var(--radius);
      background: transparent;
      color: var(--muted);
      font-size: 12px;
      cursor: pointer;
    }
    .rail-btn:hover,
    .rail-btn[aria-expanded='true'] {
      color: var(--text);
      background: var(--panel);
    }
    .scrim {
      display: block;
      position: absolute;
      inset: 0;
      z-index: 20;
      border: 0;
      background: rgba(8, 9, 12, 0.45);
      cursor: default;
    }
    .right {
      position: absolute;
      top: 0;
      right: 0;
      bottom: 0;
      z-index: 21;
      width: var(--panel-right-w);
      box-shadow: var(--shadow);
      transform: translateX(100%);
      visibility: hidden;
      transition:
        transform 0.18s ease-out,
        visibility 0s 0.18s;
    }
    .drawer-open .right {
      transform: none;
      visibility: visible;
      transition: transform 0.18s ease-out;
    }
    .drawer-close {
      display: inline-flex;
    }
  }

  /* left */
  .left {
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    background: var(--bg);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--s4) var(--s4) var(--s2);
  }
  section + section {
    margin-top: var(--s4);
  }
  .row-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
  }
  .hint.below {
    margin: var(--s2) 0 0;
  }
  .prompt {
    resize: vertical;
    min-height: 112px;
  }
  .avoid {
    resize: vertical;
    margin-top: var(--s2);
  }
  .fold {
    display: flex;
    align-items: center;
    gap: var(--s2);
    width: 100%;
    margin-top: var(--s2);
    padding: var(--s2) var(--s3);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }
  .fold span:first-of-type {
    flex: 1;
  }
  .fold:hover {
    border-color: var(--border-strong);
  }
  .caps {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s2);
    margin-top: var(--s2);
  }
  .chips {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--s2);
  }
  .chip {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    padding: 6px 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--text);
    cursor: pointer;
    font-size: 13px;
  }
  .chip small {
    font-size: 11px;
    color: var(--faint);
  }
  .chip:hover:not(.on) {
    border-color: var(--border-strong);
  }
  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .chip.on small {
    color: #c4b5fd;
  }
  .custom {
    margin-top: var(--s2);
  }
  .err-text {
    margin: var(--s1) 0 0;
    font-size: 12px;
    color: #fca5a5;
  }
  .box {
    margin: 0;
    padding: var(--s2) var(--s3);
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--panel);
    font-size: 13px;
    display: flex;
    flex-direction: column;
    gap: var(--s2);
  }
  .box p {
    margin: 0;
  }
  .box.muted {
    color: var(--muted);
  }
  .box.error {
    border-color: rgba(239, 68, 68, 0.4);
    background: var(--error-soft);
    color: #fecaca;
    overflow-wrap: anywhere;
  }
  .box-actions {
    display: flex;
    gap: var(--s2);
  }
  p.box.muted {
    flex-direction: row;
    align-items: center;
  }
  .action {
    padding: var(--s3) var(--s4) var(--s4);
    border-top: 1px solid var(--border);
    background: var(--bg);
  }
  .why {
    margin: var(--s2) 0 0;
    font-size: 12px;
    color: var(--muted);
    text-align: center;
  }
  .need {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin: 0 0 var(--s2);
    color: #fcd34d;
  }
  .resume {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin-bottom: var(--s2);
    padding: 2px 2px 2px var(--s3);
    border-radius: var(--radius);
    background: var(--accent-soft);
    color: #ddd6fe;
    font-size: 13px;
  }
  .resume span {
    flex: 1;
  }

  /* center */
  .center {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .result-head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--s2) var(--s4);
    padding: var(--s3) var(--s4);
    border-bottom: 1px solid var(--border);
  }
  .head-note {
    flex: 1 0 100%;
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .title {
    flex: 1 1 220px;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .meta {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .counters {
    display: flex;
    gap: var(--s2);
    flex-wrap: wrap;
  }
  .cnt {
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--panel-2);
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
    white-space: nowrap;
  }
  .cnt.ok {
    color: #86efac;
  }
  .cnt.run {
    color: #c4b5fd;
  }
  .cnt.warn {
    color: #fcd34d;
  }
  .cnt.fail {
    color: #fca5a5;
  }
  .outcome {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
  }
  .outcome.ok {
    color: #86efac;
  }
  .outcome.warn {
    color: #fcd34d;
  }
  .outcome.neutral {
    color: var(--muted);
  }
  .head-actions {
    display: flex;
    gap: var(--s2);
    flex-wrap: wrap;
  }
  .bar {
    height: 4px;
    background: var(--panel-2);
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.3s;
  }
  .bar span.warn {
    background: linear-gradient(90deg, var(--accent), var(--warn));
  }
  .bar span.neutral {
    background: var(--border-strong);
  }
  .grid {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, var(--cols));
    grid-auto-rows: max-content;
    align-content: start;
    gap: var(--s3);
    padding: var(--s4);
  }
  .starting {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--s2);
    color: var(--muted);
  }
  .mini-spin {
    width: 14px;
    height: 14px;
    flex: none;
    border-radius: 50%;
    border: 2px solid var(--accent-soft);
    border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* right */
  .right {
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border);
    background: var(--bg);
  }
  .hist-tools {
    display: flex;
    gap: var(--s1);
  }
  .hist-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--s3) var(--s3) var(--s2) var(--s4);
  }
  .hist {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 var(--s3) var(--s3);
  }
  .hist ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--s1);
  }
  .entry {
    width: 100%;
    display: flex;
    gap: var(--s3);
    padding: var(--s2);
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }
  .entry:hover {
    background: var(--panel);
  }
  .entry.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .thumbs {
    flex: none;
    width: 72px;
    height: 72px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-auto-rows: 1fr;
    gap: 2px;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--panel-2);
  }
  .thumbs.one {
    grid-template-columns: 1fr;
  }
  .thumbs img {
    width: 100%;
    height: 100%;
    min-height: 0;
    object-fit: cover;
    display: block;
  }
  .no-thumb {
    grid-column: 1 / -1;
    display: grid;
    place-items: center;
    color: var(--faint);
  }
  .text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .p {
    font-size: 13px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .m {
    font-size: 12px;
    color: var(--muted);
  }
  .f {
    color: #fca5a5;
  }
</style>
