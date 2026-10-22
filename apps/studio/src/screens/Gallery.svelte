<script lang="ts">
  // Gallery screen: this PC's folders (work folder, added, found by CLI runs or "이 PC에서 찾기") and other PCs' shared
  // galleries (filter, add, remove; a missing folder is shown as "찾을 수 없음"), status filter, search, records grouped
  // by day, and the selected record's details with reuse / reference / resume actions (another PC's records: view only).
  import { open } from '@tauri-apps/plugin-dialog';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import Empty from '../components/Empty.svelte';
  import GalleryCard from '../components/GalleryCard.svelte';
  import Icon from '../components/Icon.svelte';
  import Segmented from '../components/Segmented.svelte';
  import SharePanel from '../components/SharePanel.svelte';
  import Viewer, { type ViewerEntry } from '../components/Viewer.svelte';
  import { errorText, imageSrc, toServiceError } from '../lib/api';
  import { truncateMiddle } from '../lib/format';
  import { addFolder, discover, gal, loadGallery, PAGE, removeFolder, resetPage } from '../lib/gallery.svelte';
  import {
    detailRows,
    filterItems,
    folderName,
    groupByDate,
    sameTopic,
    STATUS_FILTERS,
    statusCounts,
    takeItems,
    type StatusFilter,
  } from '../lib/galleryView';
  import { openEditor } from '../lib/editor.svelte';
  import { addReferencePaths, copyPrompt, refineFromOriginal, resumeGroup, reuseItem } from '../lib/generate.svelte';
  import { resumeBlocked, samePath, splitPrompt } from '../lib/generateForm';
  import { ask, navigate, toastError } from '../lib/state.svelte';
  import type { FolderView, GalleryItem } from '../lib/types';

  void loadGallery();

  const now = new Date();
  const inFolderAndSearch = $derived(filterItems(gal.items, { folder: gal.folder, status: 'all', query: gal.query }));
  const counts = $derived(statusCounts(inFolderAndSearch));
  const filtered = $derived(filterItems(gal.items, { folder: gal.folder, status: gal.status, query: gal.query }));
  const groups = $derived(groupByDate(filtered, now));
  const visible = $derived(takeItems(groups, gal.shown));
  const left = $derived(Math.max(0, filtered.length - gal.shown));
  const currentFolder = $derived(gal.folder === null ? null : (gal.folders.find((f) => samePath(f.galleryRoot, gal.folder as string)) ?? null));
  const missingFolders = $derived(gal.folders.filter((f) => !f.available));
  const selected = $derived(gal.selected === null ? null : (gal.items.find((i) => i.recordPath === gal.selected) ?? null));
  const statusOptions = $derived(STATUS_FILTERS.map((s) => ({ value: s.value, label: `${s.label} ${counts[s.value]}` })));

  /** Another PC that could not be read has no folder path: it shows by its PC name. */
  function chipLabel(folder: FolderView): string {
    return folder.source === 'peer' && !folder.available ? (folder.machine ?? '다른 PC') : folderName(folder.path);
  }

  function chipTitle(folder: FolderView): string {
    const where = folder.source === 'peer' ? `${folder.machine ?? '다른 PC'} · ${folder.path.replace(/^peer:\/\/[^/]+\//, '')}` : folder.path;
    if (folder.available) return where;
    return folder.note ?? `찾을 수 없음: ${where}`;
  }

  function folderCount(folder: FolderView): number {
    return gal.items.filter((i) => samePath(i.folder, folder.galleryRoot)).length;
  }

  function chooseFolder(root: string | null) {
    gal.folder = root;
    resetPage();
  }

  function chooseStatus(status: StatusFilter) {
    gal.status = status;
    resetPage();
  }

  function clearFilters() {
    gal.folder = null;
    gal.status = 'all';
    gal.query = '';
    resetPage();
  }

  async function pickFolder() {
    try {
      const picked = await open({ title: '갤러리에 추가할 폴더 고르기', directory: true, multiple: false });
      if (typeof picked === 'string') await addFolder(picked);
    } catch (e) {
      toastError(toServiceError(e));
    }
  }

  async function askRemove(folder: FolderView) {
    const ok = await ask({
      title: '갤러리에서 폴더 빼기',
      message: `"${folderName(folder.path)}" 폴더를 갤러리 목록에서 뺍니다. 폴더와 이미지 파일은 지우지 않습니다. (${folder.path})`,
      confirmText: '목록에서 빼기',
    });
    if (ok) await removeFolder(folder);
  }

  async function reveal(path: string) {
    try {
      await revealItemInDir(path);
    } catch (e) {
      toastError(toServiceError(e));
    }
  }

  // ---- actions that continue on the generate screen ----
  function reuse(item: GalleryItem) {
    void reuseItem(item);
    navigate('generate');
  }

  function useAsReference(path: string) {
    void addReferencePaths([path]);
    navigate('generate');
  }

  function resume(item: GalleryItem) {
    void resumeGroup(sameTopic(gal.items, item));
    navigate('generate');
  }

  const selectedResumeBlocked = $derived(selected ? resumeBlocked(selected.folder, gal.folders) : null);

  // ---- large view over the finished images of the current filter ----
  let viewerIndex: number | null = $state(null);
  const entries: ViewerEntry[] = $derived(
    filtered
      .filter((i) => i.status === 'complete' || i.status === 'legacy')
      .map((i) => ({
        key: i.recordPath,
        imagePath: i.imagePath,
        label: '',
        transparent: i.background === 'transparent',
        item: i,
        prompt: i.prompt,
        model: i.imageModel,
        size: i.size,
      })),
  );

  function openViewer(item: GalleryItem) {
    const at = entries.findIndex((e) => e.key === item.recordPath);
    if (at >= 0) viewerIndex = at;
  }

  const selectedPrompt = $derived(selected ? splitPrompt(selected.prompt) : null);
  const selectedHasImage = $derived(selected?.status === 'complete' || selected?.status === 'legacy');
  /** A record of another PC: shown and reusable as settings, but its files are not on this PC. */
  const selectedRemote = $derived(!!selected?.machine);
  const shownPath = (path: string) => path.replace(/^peer:\/\/[^/]+\//, '');
</script>

<div class="gallery" class:with-detail={!!selected}>
  <section class="main" aria-label="갤러리">
    <header class="toolbar">
      <div class="folders" role="group" aria-label="폴더">
        <button type="button" class="chip" class:on={gal.folder === null} aria-pressed={gal.folder === null} onclick={() => chooseFolder(null)}>
          <span>전체 폴더</span><span class="n">{gal.items.length}</span>
        </button>
        {#each gal.folders as f (f.galleryRoot)}
          <span class="chip-wrap" class:on={gal.folder !== null && samePath(gal.folder, f.galleryRoot)} class:gone={!f.available}>
            <button
              type="button"
              class="chip inner"
              aria-pressed={gal.folder !== null && samePath(gal.folder, f.galleryRoot)}
              title={chipTitle(f)}
              onclick={() => chooseFolder(f.galleryRoot)}
            >
              <Icon name={f.source === 'peer' ? 'computer' : 'folder'} size={15} />
              <span class="name">{chipLabel(f)}</span>
              {#if f.isWorkDir}<span class="tag">작업 폴더</span>{/if}
              {#if f.source === 'peer' && f.available}<span class="tag pc">{f.machine}</span>{/if}
              {#if f.available}<span class="n">{folderCount(f)}</span>{:else}<span class="tag gone-tag">{f.source === 'peer' ? '연결 안 됨' : '찾을 수 없음'}</span>{/if}
            </button>
            {#if !f.isWorkDir && f.source !== 'peer'}
              <button type="button" class="x" aria-label="{folderName(f.path)} 폴더를 목록에서 빼기" title="목록에서 빼기" disabled={gal.busy} onclick={() => askRemove(f)}>
                <Icon name="close" size={13} />
              </button>
            {/if}
          </span>
        {/each}
        <Button size="sm" variant="ghost" icon="plus" disabled={gal.busy} onclick={pickFolder}>폴더 추가</Button>
        <Button
          size="sm"
          variant="ghost"
          icon="search"
          disabled={gal.discovering}
          title="이 PC의 드라이브에서 ImageGallery 폴더(에이전트·CLI가 만든 이미지)를 찾아 함께 보여 줍니다"
          onclick={() => discover()}>{gal.discovering ? '찾는 중…' : '이 PC에서 찾기'}</Button
        >
        <Button size="sm" variant="ghost" icon="computer" title="다른 PC의 갤러리 함께 보기 · 이 PC 공유" onclick={() => (gal.shareOpen = true)}>다른 PC</Button>
      </div>
      <div class="filters">
        <Segmented label="상태" options={statusOptions} value={gal.status} onchange={chooseStatus} />
        <label class="search">
          <Icon name="search" size={16} />
          <input type="search" placeholder="프롬프트·모델 검색" aria-label="프롬프트·모델 검색" bind:value={gal.query} oninput={resetPage} />
        </label>
        <span class="total muted">{filtered.length}개</span>
        <Button size="sm" variant="ghost" icon="refresh" label="새로 읽기" title="새로 읽기" disabled={gal.loading} onclick={loadGallery} />
      </div>
    </header>

    <div class="body">
      {#if gal.error && !gal.loaded}
        <Empty icon="alert" title="갤러리를 읽지 못했습니다" text={errorText(gal.error)}>
          <Button variant="primary" icon="refresh" onclick={loadGallery}>다시 시도</Button>
        </Empty>
      {:else if !gal.loaded}
        <div class="loading"><span class="mini-spin" aria-hidden="true"></span> 갤러리를 읽는 중…</div>
      {:else if currentFolder && !currentFolder.available}
        <Empty
          icon={currentFolder.source === 'peer' ? 'computer' : 'folder'}
          title={currentFolder.source === 'peer' ? '다른 PC에 연결할 수 없습니다' : '폴더를 찾을 수 없습니다'}
          text={currentFolder.source === 'peer'
            ? (currentFolder.note ?? `${currentFolder.machine}에 연결할 수 없습니다.`)
            : `${currentFolder.path} — 외장 드라이브가 빠졌거나, 폴더 이름이 바뀌었거나, 읽을 권한이 없습니다. 다시 연결한 뒤 새로 읽으세요.`}
        >
          <Button icon="refresh" onclick={loadGallery}>새로 읽기</Button>
          {#if currentFolder.source === 'peer'}
            <Button variant="ghost" icon="computer" onclick={() => (gal.shareOpen = true)}>다른 PC 설정</Button>
          {:else if !currentFolder.isWorkDir}
            <Button variant="ghost" icon="close" onclick={() => currentFolder && askRemove(currentFolder)}>목록에서 빼기</Button>
          {/if}
        </Empty>
      {:else if gal.items.length === 0}
        <Empty
          icon="gallery"
          title={gal.discovering ? '이 PC에서 갤러리 폴더를 찾는 중입니다' : '아직 이미지가 없습니다'}
          text="생성 화면에서 만든 이미지, 에이전트·CLI가 이 PC에 만든 이미지, 추가한 폴더와 다른 PC의 이미지가 여기에 날짜별로 모입니다."
        >
          <Button variant="primary" icon="generate" onclick={() => navigate('generate')}>생성 화면으로</Button>
          <Button icon="plus" disabled={gal.busy} onclick={pickFolder}>폴더 추가</Button>
          <Button icon="search" disabled={gal.discovering} onclick={() => discover()}>이 PC에서 찾기</Button>
        </Empty>
      {:else if filtered.length === 0}
        <Empty icon="search" title="조건에 맞는 이미지가 없습니다" text="폴더, 상태, 검색어를 바꿔 보세요.">
          <Button icon="close" onclick={clearFilters}>필터 지우기</Button>
        </Empty>
      {:else}
        <div class="scroll">
          {#if gal.error}
            <p class="band error"><Icon name="alert" size={16} /> 새로 읽지 못해 이전 목록을 보여 줍니다. {errorText(gal.error)}</p>
          {/if}
          {#if gal.discovering}
            <p class="band"><span class="mini-spin" aria-hidden="true"></span> 이 PC의 드라이브에서 갤러리 폴더를 찾는 중입니다. 찾는 대로 목록에 더합니다.</p>
          {/if}
          {#if gal.folder === null && missingFolders.length > 0}
            <p class="band warn">
              <Icon name="alert" size={16} />
              찾을 수 없거나 연결되지 않은 곳 {missingFolders.length}개: {missingFolders.map((f) => chipLabel(f)).join(', ')} — 나머지는 그대로 보여 줍니다.
            </p>
          {/if}
          {#if gal.truncated}
            <p class="band"><Icon name="info" size={16} /> 기록이 많아 최근 것부터 일부만 읽었습니다.</p>
          {/if}
          {#each visible as day (day.key)}
            <section class="day" aria-label={day.label}>
              <h3>{day.label} <span class="muted">{groups.find((g) => g.key === day.key)?.items.length ?? day.items.length}개</span></h3>
              <div class="grid">
                {#each day.items as item (item.recordPath)}
                  <GalleryCard
                    {item}
                    selected={gal.selected === item.recordPath}
                    onselect={() => (gal.selected = item.recordPath)}
                    onopen={() => openViewer(item)}
                  />
                {/each}
              </div>
            </section>
          {/each}
          {#if left > 0}
            <div class="more">
              <Button icon="chevronDown" onclick={() => (gal.shown += PAGE)}>더 보기 ({left}개 남음)</Button>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </section>

  {#if selected && selectedPrompt}
    <aside class="detail" aria-label="자세히">
      <header class="detail-head">
        <h2 class="detail-title">자세히</h2>
        <Button size="sm" variant="ghost" icon="close" label="자세히 닫기" title="닫기" onclick={() => (gal.selected = null)} />
      </header>
      <div class="detail-scroll">
        {#if selectedHasImage}
          <button type="button" class="preview" class:checker={selected.background === 'transparent'} title="크게 보기" onclick={() => openViewer(selected)}>
            <img src={imageSrc(selected.imagePath, true)} alt="선택한 이미지" />
          </button>
        {/if}
        {#if selected.originalPrompt}
          <h2>원래 요청</h2>
          <p class="prompt">{selected.originalPrompt}</p>
          <div class="minor">
            <Button size="sm" icon="copy" onclick={() => selected.originalPrompt && copyPrompt(selected.originalPrompt)}>원래 요청 복사</Button>
            <Button size="sm" icon="edit" onclick={() => refineFromOriginal(selected)}>이걸로 다시 다듬기</Button>
          </div>
          <h2>보낸 프롬프트(영어로 다듬음)</h2>
        {:else}
          <h2>프롬프트</h2>
        {/if}
        <p class="prompt">{selectedPrompt.prompt || '(없음)'}</p>
        {#if selectedPrompt.avoid}
          <h2>Avoid</h2>
          <p class="prompt">{selectedPrompt.avoid}</p>
        {/if}
        <dl>
          {#each detailRows(selected, now) as row (row.label)}
            <dt>{row.label}</dt>
            <dd class:fail={row.label === '실패 원인'} class:mono={row.label === '요청 ID'}>
              {#if row.label === '상태'}
                {#if selected.status === 'failed'}<Badge tone="error">{row.value}</Badge>
                {:else if selected.status === 'pending'}<Badge tone="warn">{row.value}</Badge>
                {:else if selected.status === 'missing'}<Badge>{row.value}</Badge>
                {:else}<Badge tone="ok">{row.value}</Badge>{/if}
              {:else}
                {row.value}
              {/if}
            </dd>
          {/each}
          {#if selected.machine}
            <dt>PC</dt>
            <dd>{selected.machine}</dd>
          {/if}
          <dt>폴더</dt>
          <dd class="path" title={shownPath(selected.folder)}>{truncateMiddle(shownPath(selected.folder), 48)}</dd>
          <dt>파일</dt>
          <dd class="path mono" title={shownPath(selected.imagePath)}>{truncateMiddle(shownPath(selected.imagePath), 64)}</dd>
        </dl>
      </div>
      <div class="actions">
        <Button variant="primary" icon="refresh" full onclick={() => reuse(selected)}>이 설정 다시 쓰기</Button>
        <div class="minor">
          <Button size="sm" icon="copy" title="프롬프트 복사" onclick={() => copyPrompt(selected.prompt)}>복사</Button>
          {#if !selectedRemote}
            <Button size="sm" icon="folder" title="탐색기에서 보기" onclick={() => reveal(selectedHasImage ? selected.imagePath : selected.recordPath)}>탐색기</Button>
          {/if}
          {#if selectedHasImage && !selectedRemote}
            <Button size="sm" icon="edit" title="편집 · 배경 지우기" onclick={() => openEditor(selected.imagePath)}>편집</Button>
            <Button size="sm" icon="image" title="참조로 사용" onclick={() => useAsReference(selected.imagePath)}>참조로</Button>
          {/if}
          <Button
            size="sm"
            icon="play"
            disabled={!!selectedResumeBlocked}
            title={selectedResumeBlocked ?? '이어서 생성'}
            onclick={() => resume(selected)}>이어서</Button
          >
        </div>
        {#if selectedRemote}
          <p class="note">{selected.machine}의 이미지입니다. 이 PC에서는 보기와 설정 다시 쓰기만 할 수 있습니다.</p>
        {:else if selectedResumeBlocked}<p class="note">{selectedResumeBlocked}</p>{/if}
      </div>
    </aside>
  {/if}
</div>

{#if viewerIndex !== null && entries.length > 0}
  <Viewer
    {entries}
    bind:index={viewerIndex}
    onclose={() => (viewerIndex = null)}
    onreuse={(item) => {
      viewerIndex = null;
      reuse(item);
    }}
    onresume={(item) => {
      viewerIndex = null;
      resume(item);
    }}
    resumeNote={(item) => resumeBlocked(item.folder, gal.folders)}
    onreference={(path) => {
      viewerIndex = null;
      useAsReference(path);
    }}
  />
{/if}

<SharePanel open={gal.shareOpen} onclose={() => (gal.shareOpen = false)} />

<style>
  .gallery {
    height: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
  }
  /* The detail column opens only while a record is selected. */
  .gallery.with-detail {
    grid-template-columns: minmax(0, 1fr) var(--panel-right-w);
  }
  .main {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* toolbar */
  .toolbar {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    padding: var(--s3) var(--s4);
    border-bottom: 1px solid var(--border);
  }
  .folders,
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s2);
  }
  .chip,
  .chip-wrap {
    display: inline-flex;
    align-items: center;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--panel);
    color: var(--text);
    font-size: 13px;
    max-width: 320px;
  }
  .chip {
    gap: var(--s2);
    padding: 0 var(--s3);
    cursor: pointer;
  }
  .chip-wrap .chip.inner {
    height: 100%;
    border: 0;
    background: transparent;
    min-width: 0;
    border-radius: 999px;
  }
  .chip:hover,
  .chip-wrap:hover {
    border-color: var(--border-strong);
  }
  .chip.on,
  .chip-wrap.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .chip-wrap .chip.inner {
    border-color: transparent;
    background: transparent;
  }
  .chip-wrap.gone {
    color: var(--faint);
    border-style: dashed;
  }
  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .n {
    font-size: 12px;
    color: var(--muted);
  }
  .tag {
    flex: none;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--panel-3);
    font-size: 11px;
    color: var(--muted);
  }
  .gone-tag {
    color: #fcd34d;
    background: var(--warn-soft);
  }
  .tag.pc {
    color: var(--text);
    background: var(--accent-soft);
  }
  .x {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin-right: 3px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .x:hover:not(:disabled) {
    background: var(--panel-3);
    color: var(--text);
  }
  .search {
    flex: 1 1 200px;
    max-width: 360px;
    display: flex;
    align-items: center;
    gap: var(--s2);
    height: 34px;
    padding: 0 var(--s3);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel-2);
    color: var(--muted);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    outline: none;
  }
  .search input:focus-visible {
    outline: none;
  }
  .search:has(input:focus-visible) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .search input::placeholder {
    color: var(--faint);
  }
  .total {
    font-size: 12px;
    margin-left: auto;
  }

  /* body */
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--s3) var(--s4) var(--s5);
  }
  .band {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin: 0 0 var(--s3);
    padding: var(--s2) var(--s3);
    border-radius: var(--radius);
    background: var(--panel);
    border: 1px solid var(--border);
    font-size: 13px;
    color: var(--muted);
  }
  .band.warn {
    background: var(--warn-soft);
    border-color: rgba(245, 158, 11, 0.4);
    color: #fcd34d;
  }
  .band.error {
    background: var(--error-soft);
    border-color: rgba(239, 68, 68, 0.4);
    color: #fecaca;
  }
  .day + .day {
    margin-top: var(--s4);
  }
  h3 {
    margin: 0 0 var(--s2);
    font-size: 13px;
    font-weight: 600;
  }
  h3 .muted {
    font-weight: 400;
    margin-left: var(--s1);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(176px, 1fr));
    gap: var(--s3);
  }
  .more {
    display: flex;
    justify-content: center;
    margin-top: var(--s4);
  }
  .loading {
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
    border-radius: 50%;
    border: 2px solid var(--accent-soft);
    border-top-color: var(--accent);
    animation: spin 1.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* detail */
  .detail {
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border);
    background: var(--bg);
  }
  .detail-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--s4);
  }
  .preview {
    display: block;
    width: 100%;
    padding: 0;
    margin-bottom: var(--s3);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    cursor: zoom-in;
  }
  .preview:not(.checker) {
    background: var(--panel);
  }
  .preview:hover {
    border-color: var(--accent-line);
  }
  .preview img {
    width: 100%;
    max-height: 240px;
    object-fit: contain;
    display: block;
  }
  h2 {
    margin: var(--s3) 0 var(--s1);
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }
  .prompt {
    margin: 0;
    font-size: 13px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 220px;
    overflow: auto;
  }
  dl {
    display: grid;
    grid-template-columns: 76px minmax(0, 1fr);
    gap: 6px var(--s2);
    margin: var(--s4) 0 0;
    font-size: 13px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  dd.fail {
    color: #fecaca;
  }
  .mono {
    font-family: var(--mono);
    font-size: 12px;
  }
  .path {
    color: var(--muted);
  }
  .detail-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--s2) var(--s2) 0 var(--s4);
  }
  .detail-title {
    margin: 0;
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    padding: var(--s3) var(--s4) var(--s4);
    border-top: 1px solid var(--border-strong);
    background: var(--bg);
    /* The fixed footer casts a shadow upward, so cut-off details read as "scroll for more". */
    box-shadow: 0 -10px 16px rgba(0, 0, 0, 0.35);
    position: relative;
  }
  .minor {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(76px, 1fr));
    gap: var(--s2);
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
</style>
