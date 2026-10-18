<script lang="ts">
  // Connections screen: saved connections and the environment connection on the left, the edit form and check result on the right.
  // Keys are typed into a password field, sent once to save_profile and cleared; nothing here can read a stored key back.
  import Badge from '../components/Badge.svelte';
  import Button from '../components/Button.svelte';
  import CheckResult from '../components/CheckResult.svelte';
  import Empty from '../components/Empty.svelte';
  import Icon from '../components/Icon.svelte';
  import Segmented from '../components/Segmented.svelte';
  import StageBadges from '../components/StageBadges.svelte';
  import * as api from '../lib/api';
  import { ENV_NAME, stagesOf } from '../lib/connection';
  import {
    KINDS,
    MAX_CONCURRENCY,
    changeKind,
    formFromProfile,
    hasErrors,
    isDirty,
    keyModes,
    kindInfo,
    newForm,
    officialUrl,
    toInput,
    transportWarning,
    urlLocked,
    validate,
    type ConnectionForm,
    type FormErrors,
    type KeyMode,
  } from '../lib/connectionsForm';
  import { truncateMiddle } from '../lib/format';
  import { app, ask, checkConnection, refreshConnections, toast, toastError } from '../lib/state.svelte';
  import type { ProfileView, ProviderKind, ServiceError } from '../lib/types';

  type Selection = { type: 'profile'; id: string } | { type: 'env' } | { type: 'new' };

  const profiles = $derived(app.connections?.profiles ?? []);
  const env = $derived(app.connections?.env ?? null);
  const active = $derived(app.connections?.active ?? null);

  function firstSelection(): Selection {
    const list = app.connections?.profiles ?? [];
    const def = list.find((p) => p.isDefault) ?? list[0];
    if (def) return { type: 'profile', id: def.id };
    if (app.connections?.env) return { type: 'env' };
    return { type: 'new' };
  }

  function formFor(s: Selection): ConnectionForm {
    if (s.type === 'profile') {
      const p = (app.connections?.profiles ?? []).find((x) => x.id === s.id);
      if (p) return formFromProfile(p);
    }
    // The first connection becomes the default one.
    return { ...newForm(), makeDefault: (app.connections?.profiles.length ?? 0) === 0 };
  }

  const start = firstSelection();
  let sel: Selection = $state(start);
  let form: ConnectionForm = $state(formFor(start));
  let attempted = $state(false);
  let touched: Partial<Record<keyof FormErrors, boolean>> = $state({});
  let saving = $state(false);
  let saveError: ServiceError | null = $state(null);
  let checking: Record<string, boolean> = $state({});
  let checkedAt: Record<string, number> = $state({});

  const selectedId = $derived(sel.type === 'profile' ? sel.id : null);
  const saved: ProfileView | null = $derived(profiles.find((p) => p.id === selectedId) ?? null);
  const errors = $derived(validate(form, profiles));
  const dirty = $derived(sel.type !== 'env' && isDirty(form, saved));
  const info = $derived(kindInfo(form.kind));
  const locked = $derived(urlLocked(form));
  const httpWarning = $derived(!locked && transportWarning(form.baseUrl));
  const isNew = $derived(form.id === null);
  const firstRun = $derived(!!app.connections && profiles.length === 0 && !env);

  function shown(field: keyof FormErrors): string | undefined {
    return attempted || touched[field] ? errors[field] : undefined;
  }

  function checkKey(name: string | null): string {
    return name ?? 'env';
  }

  function stagesFor(p: ProfileView) {
    return stagesOf(app.checks[p.name] ?? null, p.lastGeneration);
  }

  async function select(next: Selection) {
    if (sel.type === next.type && (next.type !== 'profile' || (sel.type === 'profile' && sel.id === next.id))) return;
    if (dirty) {
      const ok = await ask({
        title: '저장하지 않은 변경',
        message: '바꾼 내용을 저장하지 않았습니다. 버리고 이동할까요?',
        confirmText: '버리고 이동',
        cancelText: '계속 편집',
        danger: true,
      });
      if (!ok) return;
    }
    sel = next;
    form = formFor(next);
    attempted = false;
    touched = {};
    saveError = null;
  }

  function onKind(kind: ProviderKind) {
    form = changeKind(form, kind);
  }

  function setKeyMode(mode: KeyMode) {
    form.keyMode = mode;
    if (mode !== 'replace') form.key = '';
  }

  async function save() {
    attempted = true;
    saveError = null;
    if (hasErrors(errors)) return;
    const urlChanged = !saved || form.baseUrl.trim() !== saved.baseUrl;
    if (httpWarning && urlChanged) {
      const ok = await ask({
        title: '암호화되지 않은 주소',
        message:
          `${form.baseUrl.trim()}\n\nhttp:// 주소는 키와 이미지가 암호화되지 않고 그대로 전송됩니다. ` +
          '공용 인터넷 주소라면 https 주소를 쓰거나 Tailscale·사설망 안에서만 쓰세요.',
        confirmText: '그래도 저장',
        cancelText: '돌아가기',
        danger: true,
      });
      if (!ok) return;
    }
    const before = saved;
    saving = true;
    try {
      const view = await api.saveProfile(toInput(form));
      // Address or key may have changed: earlier check results no longer describe this connection.
      if (before) delete app.checks[before.name];
      delete app.checks[view.name];
      await refreshConnections();
      sel = { type: 'profile', id: view.id };
      form = formFor(sel);
      attempted = false;
      touched = {};
      toast(before ? `"${view.name}" 연결을 저장했습니다.` : `"${view.name}" 연결을 추가했습니다. 연결 확인으로 상태를 볼 수 있습니다.`, 'ok');
    } catch (e) {
      saveError = api.toServiceError(e);
    } finally {
      saving = false;
    }
  }

  async function remove() {
    if (!saved) return;
    const p = saved;
    const ok = await ask({
      title: '연결 삭제',
      message: `"${p.name}" 연결과 자격 증명 관리자에 저장된 키를 지웁니다. 만든 이미지는 지워지지 않습니다.`,
      confirmText: '삭제',
      danger: true,
    });
    if (!ok) return;
    try {
      await api.removeProfile(p.id);
      delete app.checks[p.name];
      await refreshConnections();
      sel = firstSelection();
      form = formFor(sel);
      attempted = false;
      touched = {};
      saveError = null;
      toast(`"${p.name}" 연결을 삭제했습니다.`, 'ok');
    } catch (e) {
      toastError(api.toServiceError(e));
    }
  }

  async function setDefault(id: string | null) {
    try {
      await api.setDefaultProfile(id);
      await refreshConnections();
      toast(id ? '기본 연결로 지정했습니다.' : '기본 연결 지정을 해제했습니다.', 'ok');
    } catch (e) {
      toastError(api.toServiceError(e));
    }
  }

  async function check(name: string | null) {
    const key = checkKey(name);
    checking[key] = true;
    try {
      const report = await checkConnection(name);
      if (report) checkedAt[key] = Date.now() / 1000;
    } finally {
      checking[key] = false;
    }
  }

  async function retryLoad() {
    await refreshConnections();
    if (app.connections) {
      sel = firstSelection();
      form = formFor(sel);
    }
  }

  const SOURCE: Record<string, string> = { process_env: '앱을 실행한 환경', user_env: 'Windows 사용자 환경변수' };
</script>

{#if !app.connections && app.connectionsError}
  <Empty icon="alert" title="연결 정보를 읽지 못했습니다" text={api.errorText(app.connectionsError)}>
    <Button variant="primary" icon="refresh" onclick={retryLoad}>다시 읽기</Button>
  </Empty>
{:else}
  <div class="screen">
    <aside class="list" aria-label="연결 목록">
      <header>
        <h2>연결</h2>
        <Button variant="primary" size="sm" icon="plus" onclick={() => select({ type: 'new' })}>새 연결</Button>
      </header>

      {#if app.connectionsError}
        <div class="band error" role="alert">
          <Icon name="alert" size={16} />
          <span>최신 연결 정보를 읽지 못했습니다. {api.errorText(app.connectionsError)}</span>
          <Button size="sm" variant="ghost" icon="refresh" onclick={retryLoad} label="다시 읽기" />
        </div>
      {/if}

      <div class="items">
        {#if env}
          <button
            type="button"
            class="item"
            class:on={sel.type === 'env'}
            aria-current={sel.type === 'env' ? 'true' : undefined}
            onclick={() => select({ type: 'env' })}
          >
            <div class="top">
              <Icon name="terminal" size={16} />
              <span class="name ellipsis">{ENV_NAME}</span>
              {#if active === 'env'}<Badge tone="ok">사용 중</Badge>{/if}
              <Badge>읽기 전용</Badge>
            </div>
            <div class="meta ellipsis" title={env.baseUrl}>CLIProxyAPI · {truncateMiddle(env.baseUrl, 40)}</div>
            <div class="bottom">
              <StageBadges stages={stagesOf(app.checks.env ?? null, null)} compact />
              <span class="keystate" class:missing={!env.keyConfigured}>{env.keyConfigured ? '키 있음' : '키 없음'}</span>
            </div>
          </button>
        {/if}

        {#each profiles as p (p.id)}
          <button
            type="button"
            class="item"
            class:on={sel.type === 'profile' && sel.id === p.id}
            aria-current={sel.type === 'profile' && sel.id === p.id ? 'true' : undefined}
            onclick={() => select({ type: 'profile', id: p.id })}
          >
            <div class="top">
              <span class="name ellipsis" title={p.name}>{p.name}</span>
              {#if p.isDefault}<Badge tone="accent" title="이 연결이 기본 연결입니다"><Icon name="star" size={12} />기본</Badge>{/if}
              {#if active === p.name}<Badge tone="ok">사용 중</Badge>{/if}
            </div>
            <div class="meta ellipsis" title={p.baseUrl}>{p.kindLabel} · {truncateMiddle(p.baseUrl, 40)}</div>
            <div class="bottom">
              <StageBadges stages={stagesFor(p)} compact />
              {#if p.warning}<span class="warnmark" title={p.warning}><Icon name="alert" size={14} label="암호화되지 않은 주소" /></span>{/if}
              <span class="keystate" class:missing={!p.keyConfigured && kindInfo(p.kind).keyRequired}>
                <Icon name="key" size={13} />{p.keyConfigured ? '키 저장됨' : '키 없음'}
              </span>
            </div>
          </button>
        {/each}

        {#if profiles.length === 0 && !env}
          <p class="empty-list">
            아직 연결이 없습니다. 오른쪽에서 첫 연결을 추가하세요. 이 PC의 CLIProxyAPI라면 주소는 보통 http://127.0.0.1:8317/v1입니다.
          </p>
        {/if}

        {#if sel.type === 'new' && profiles.length > 0}
          <div class="item on ghost-item" aria-current="true">
            <div class="top"><Icon name="plus" size={16} /><span class="name">새 연결</span></div>
            <div class="meta">아직 저장하지 않음</div>
          </div>
        {/if}
      </div>

      {#if env && profiles.some((p) => p.isDefault)}
        <p class="order">
          <Icon name="info" size={14} />
          <span>환경변수 연결이 있으면 기본 연결보다 먼저 쓰입니다. 생성 화면에서 연결을 직접 고르면 그 연결을 씁니다.</span>
        </p>
      {/if}
    </aside>

    <section class="detail">
      {#if sel.type === 'env' && env}
        <div class="pane">
          <header class="head">
            <div>
              <h2>{ENV_NAME}</h2>
              <p class="muted">CLIPROXY_BASE_URL 환경변수로 만든 연결입니다. 이 앱에서는 바꿀 수 없습니다.</p>
            </div>
            {#if active === 'env'}<Badge tone="ok">지금 사용 중</Badge>{/if}
          </header>
          <dl class="facts">
            <dt>종류</dt>
            <dd>CLIProxyAPI</dd>
            <dt>주소</dt>
            <dd class="mono">{env.baseUrl}</dd>
            <dt>가져온 곳</dt>
            <dd>{SOURCE[env.source] ?? env.source}</dd>
            <dt>키</dt>
            <dd>{env.keyConfigured ? 'CLIPROXY_API_KEY(또는 OPENAI_API_KEY)에서 읽음' : '키 환경변수가 없습니다'}</dd>
          </dl>
          <div class="checkbar">
            <h3>연결 상태</h3>
            <Button icon="refresh" onclick={() => check(null)} disabled={checking.env}>{checking.env ? '확인 중…' : '연결 확인'}</Button>
          </div>
          <CheckResult report={app.checks.env ?? null} lastGeneration={null} checking={!!checking.env} />
        </div>
      {:else}
        <form
          class="pane"
          onsubmit={(e) => {
            e.preventDefault();
            save();
          }}
          autocomplete="off"
          novalidate
        >
          <header class="head">
            <div>
              {#if isNew}
                <h2>새 연결</h2>
                <p class="muted">
                  {#if firstRun}이미지를 만들려면 연결이 하나 필요합니다. 종류를 고르고 주소와 키를 넣으세요.{:else}종류를 고르고 주소와 키를 넣으세요.{/if}
                </p>
              {:else if saved}
                <h2 class="ellipsis" title={saved.name}>{saved.name}</h2>
                <p class="muted">
                  {[saved.kindLabel, saved.isDefault ? '기본 연결' : '', active === saved.name ? '지금 사용 중' : ''].filter(Boolean).join(' · ')}
                </p>
              {/if}
            </div>
            {#if saved}
              <div class="head-actions">
                {#if saved.isDefault}
                  <Button size="sm" variant="ghost" onclick={() => setDefault(null)}>기본 해제</Button>
                {:else}
                  <Button size="sm" icon="star" onclick={() => setDefault(saved.id)}>기본으로 지정</Button>
                {/if}
                <Button size="sm" variant="danger" icon="trash" onclick={remove}>삭제</Button>
              </div>
            {/if}
          </header>

          {#if saved}
            <div class="status">
              <div class="checkbar">
                <h3>연결 상태</h3>
                <Button icon="refresh" onclick={() => check(saved.name)} disabled={!!checking[saved.name]}>
                  {checking[saved.name] ? '확인 중…' : '연결 확인'}
                </Button>
              </div>
              {#if dirty}<p class="hint">저장된 설정으로 확인합니다. 바꾼 내용은 저장한 뒤 확인하세요.</p>{/if}
              <CheckResult report={app.checks[saved.name] ?? null} lastGeneration={saved.lastGeneration} checking={!!checking[saved.name]} />
            </div>
            <h3 class="section">설정</h3>
          {/if}

          <fieldset class="group">
            <legend class="label">종류</legend>
            {#if isNew}
              <div class="kinds" role="radiogroup" aria-label="연결 종류">
                {#each KINDS as k (k.kind)}
                  <button
                    type="button"
                    role="radio"
                    aria-checked={form.kind === k.kind}
                    class="kind"
                    class:on={form.kind === k.kind}
                    onclick={() => onKind(k.kind)}
                  >
                    <span class="kname">{k.label}</span>
                    <span class="kurl">{k.officialUrl ? '공식 주소' : '주소 직접 입력'}</span>
                  </button>
                {/each}
              </div>
              <p class="hint">{info.hint}</p>
            {:else}
              <p class="fixed">
                <Icon name="lock" size={14} />
                <span>{info.label}</span>
                <span class="muted">· 종류는 바꿀 수 없습니다. 다른 종류는 새 연결로 추가하세요.</span>
              </p>
            {/if}
          </fieldset>

          <div class="grid2">
            <div class="group">
              <label class="label" for="c-name">이름</label>
              <input
                id="c-name"
                autocomplete="off"
                class="field"
                class:invalid={!!shown('name')}
                maxlength="80"
                placeholder={form.kind === 'cliproxy' ? '예: local, 집 데스크톱' : '예: 회사 OpenAI'}
                bind:value={form.name}
                onblur={() => (touched.name = true)}
                aria-invalid={!!shown('name')}
                aria-describedby={shown('name') ? 'c-name-err' : undefined}
              />
              {#if shown('name')}<p class="err" id="c-name-err">{shown('name')}</p>{/if}
            </div>

            <div class="group">
              <label class="label" for="c-conc">동시 실행 수</label>
              <input
                id="c-conc"
                autocomplete="off"
                class="field"
                class:invalid={!!shown('concurrency')}
                inputmode="numeric"
                placeholder={`기본값 ${info.defaultConcurrency}`}
                bind:value={form.concurrency}
                onblur={() => (touched.concurrency = true)}
                aria-invalid={!!shown('concurrency')}
                aria-describedby="c-conc-hint"
              />
              {#if shown('concurrency')}
                <p class="err" id="c-conc-hint">{shown('concurrency')}</p>
              {:else}
                <p class="hint" id="c-conc-hint">한 번에 함께 만드는 장수(1~{MAX_CONCURRENCY}). 비워 두면 기본값입니다.</p>
              {/if}
            </div>
          </div>

          <div class="group">
            <label class="label" for="c-url">주소</label>
            <div class="url-row">
              <input
                id="c-url"
                autocomplete="off"
                class="field mono"
                class:invalid={!!shown('baseUrl')}
                readonly={locked}
                placeholder={form.kind === 'cliproxy' ? 'http://127.0.0.1:8317/v1' : 'https://example.com/v1'}
                spellcheck="false"
                bind:value={form.baseUrl}
                onblur={() => (touched.baseUrl = true)}
                aria-invalid={!!shown('baseUrl')}
                aria-describedby="c-url-hint"
              />
              {#if locked}
                <Button icon="edit" onclick={() => (form.urlUnlocked = true)}>주소 바꾸기</Button>
              {:else if officialUrl(form.kind)}
                <Button
                  variant="ghost"
                  icon="refresh"
                  onclick={() => {
                    form.baseUrl = officialUrl(form.kind) ?? '';
                    form.urlUnlocked = false;
                  }}>공식 주소로</Button
                >
              {/if}
            </div>
            {#if shown('baseUrl')}
              <p class="err" id="c-url-hint">{shown('baseUrl')}</p>
            {:else if locked}
              <p class="hint" id="c-url-hint">공식 주소를 씁니다. 회사 게이트웨이 등 다른 주소가 필요할 때만 바꾸세요.</p>
            {:else if form.kind === 'gemini'}
              <p class="hint" id="c-url-hint">호스트 주소만 넣습니다(/v1beta는 앱이 붙입니다).</p>
            {:else}
              <p class="hint" id="c-url-hint">끝에 /v1이 없으면 앱이 붙입니다. Tailscale 주소(100.x.x.x)도 됩니다.</p>
            {/if}
            {#if httpWarning}
              <div class="band warn" role="note">
                <Icon name="alert" size={16} />
                <span>암호화되지 않은 http:// 공용 주소입니다. 키와 이미지가 그대로 전송될 수 있습니다. https 주소를 쓰거나 Tailscale·사설망 안에서만 쓰세요.</span>
              </div>
            {/if}
          </div>

          <div class="group">
            <span class="label" id="c-key-label">키</span>
            {#if !isNew}
              <div class="keymode">
                <Segmented label="키 처리" options={keyModes(form)} value={form.keyMode} onchange={setKeyMode} />
              </div>
            {/if}
            {#if isNew || form.keyMode === 'replace'}
              <input
                id="c-key"
                class="field mono"
                class:invalid={!!shown('key')}
                type="password"
                autocomplete="new-password"
                spellcheck="false"
                placeholder={info.keyRequired ? 'API 키 붙여넣기' : '키가 필요 없으면 비워 두세요'}
                aria-labelledby="c-key-label"
                bind:value={form.key}
                onblur={() => (touched.key = true)}
                aria-invalid={!!shown('key')}
                aria-describedby="c-key-hint"
              />
            {/if}
            {#if shown('key')}
              <p class="err" id="c-key-hint">{shown('key')}</p>
            {:else if !isNew && form.keyMode === 'clear'}
              <p class="hint warn-text" id="c-key-hint">
                저장하면 자격 증명 관리자에서 키를 지웁니다.{#if info.keyRequired} 키가 없으면 이 연결로 이미지를 만들 수 없습니다.{/if}
              </p>
            {:else if !isNew && form.keyMode === 'keep'}
              <p class="hint" id="c-key-hint">
                {form.keyConfigured ? '저장된 키를 그대로 씁니다. 키는 화면에 다시 보이지 않습니다.' : '저장된 키가 없습니다.'}
              </p>
            {:else}
              <p class="hint" id="c-key-hint">키는 Windows 자격 증명 관리자에만 저장되고 화면·파일에는 남지 않습니다.</p>
            {/if}
          </div>

          {#if isNew}
            <label class="check-line">
              <input type="checkbox" bind:checked={form.makeDefault} />
              <span>기본 연결로 지정 <span class="muted">(생성 화면에서 따로 고르지 않으면 이 연결을 씁니다)</span></span>
            </label>
          {/if}

          {#if saveError}
            <div class="band error" role="alert">
              <Icon name="alert" size={16} />
              <span>저장하지 못했습니다. {api.errorText(saveError)}</span>
            </div>
          {/if}

          <div class="actions">
            {#if !isNew && dirty}<span class="muted dirty">저장하지 않은 변경이 있습니다</span>{/if}
            {#if isNew && profiles.length > 0}
              <Button variant="ghost" onclick={() => select(firstSelection())}>취소</Button>
            {/if}
            <Button type="submit" variant="primary" icon="check" disabled={saving || (!isNew && !dirty)}>
              {saving ? '저장 중…' : isNew ? '연결 추가' : '저장'}
            </Button>
          </div>

          {#if !saved}
            <p class="hint">저장한 뒤 서버·키·모델 목록을 확인할 수 있습니다. 확인은 이미지를 만들지 않아 비용이 들지 않습니다.</p>
          {/if}
        </form>
      {/if}
    </section>
  </div>
{/if}

<style>
  .screen {
    height: 100%;
    display: grid;
    grid-template-columns: 330px minmax(0, 1fr);
    min-height: 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
    background: var(--panel);
  }
  .list > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--s4) var(--s4) var(--s3);
  }
  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .items {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    padding: 0 var(--s3) var(--s3);
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    padding: var(--s3);
    text-align: left;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: pointer;
  }
  .item:hover {
    border-color: var(--border-strong);
  }
  .item.on {
    border-color: var(--accent);
    background: rgba(139, 92, 246, 0.08);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .empty-list {
    margin: 0;
    padding: var(--s3);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius);
    font-size: 13px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .ghost-item {
    cursor: default;
    border-style: dashed;
  }
  .top {
    display: flex;
    align-items: center;
    gap: var(--s2);
    min-width: 0;
  }
  .top .name {
    flex: 1;
    font-weight: 600;
  }
  .meta {
    font-size: 12px;
    color: var(--muted);
  }
  .bottom {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
  .keystate {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }
  .keystate.missing {
    color: #fcd34d;
  }
  .warnmark {
    color: var(--warn);
    display: inline-flex;
  }
  .order {
    display: flex;
    gap: var(--s2);
    margin: 0;
    padding: var(--s3) var(--s4);
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--muted);
  }
  .order :global(svg) {
    margin-top: 2px;
  }

  .detail {
    min-height: 0;
    overflow: auto;
  }
  .pane {
    max-width: 820px;
    display: flex;
    flex-direction: column;
    gap: var(--s5);
    padding: var(--s5) var(--s6) var(--s6);
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--s4);
  }
  .head > div:first-child {
    min-width: 0;
  }
  .head p {
    margin: 4px 0 0;
  }
  .head-actions {
    display: flex;
    gap: var(--s2);
    flex: none;
  }
  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  legend {
    padding: 0;
  }
  .group {
    min-width: 0;
  }
  .grid2 {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 220px;
    gap: var(--s4);
  }
  .kinds {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: var(--s2);
  }
  .kind {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: var(--s3);
    text-align: left;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: pointer;
    min-width: 0;
  }
  .kind:hover {
    border-color: var(--border-strong);
  }
  .kind.on {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
    background: rgba(139, 92, 246, 0.08);
  }
  .kname {
    font-weight: 600;
    font-size: 13px;
  }
  .kurl {
    font-size: 12px;
    color: var(--muted);
  }
  .fixed {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin: 0;
    flex-wrap: wrap;
  }
  .url-row {
    display: flex;
    gap: var(--s2);
  }
  .url-row .field {
    flex: 1;
  }
  .field[readonly] {
    color: var(--muted);
    background: var(--panel);
  }
  .mono {
    font-family: var(--mono);
    font-size: 13px;
  }
  .field.invalid {
    border-color: var(--error);
  }
  .hint {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .warn-text {
    color: #fcd34d;
  }
  .err {
    margin: 6px 0 0;
    font-size: 12px;
    color: #fca5a5;
  }
  .keymode {
    max-width: 420px;
    margin-bottom: var(--s2);
  }
  .check-line {
    display: flex;
    align-items: center;
    gap: var(--s2);
    cursor: pointer;
  }
  .check-line input {
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
  }
  .band {
    display: flex;
    align-items: flex-start;
    gap: var(--s2);
    margin-top: var(--s2);
    padding: var(--s2) var(--s3);
    border-radius: var(--radius);
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .band :global(svg) {
    margin-top: 2px;
  }
  .band.warn {
    background: var(--warn-soft);
    border: 1px solid rgba(245, 158, 11, 0.45);
    color: #fcd34d;
  }
  .band.error {
    background: var(--error-soft);
    border: 1px solid rgba(239, 68, 68, 0.4);
    color: #fca5a5;
  }
  .list .band {
    margin: 0 var(--s3) var(--s3);
    align-items: center;
  }
  .list .band span {
    flex: 1;
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--s2);
    padding-top: var(--s4);
    border-top: 1px solid var(--border);
  }
  .dirty {
    margin-right: auto;
    font-size: 13px;
  }
  .status {
    display: flex;
    flex-direction: column;
    gap: var(--s3);
  }
  .section {
    padding-top: var(--s4);
    border-top: 1px solid var(--border);
    margin-bottom: calc(var(--s2) * -1);
  }
  .checkbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s3);
  }
  .facts {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    gap: var(--s2) var(--s4);
    margin: 0;
    padding: var(--s4);
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
