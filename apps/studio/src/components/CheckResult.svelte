<script lang="ts">
  // Result of a connection check: the four stages with reasons, the error with its likely cause, and the image models found.
  import { stagesOf, type StageState } from '../lib/connection';
  import { checkHint } from '../lib/connectionsForm';
  import { categoryLabel, formatRelativeTime } from '../lib/format';
  import type { ConnectionReport, EngineError, GenerationStatus } from '../lib/types';
  import Icon from './Icon.svelte';

  interface Props {
    /** null = not checked yet in this session. */
    report: ConnectionReport | null;
    lastGeneration: GenerationStatus | null;
    checking?: boolean;
  }

  let { report, lastGeneration, checking = false }: Props = $props();

  const stages = $derived(stagesOf(report, lastGeneration));
  const WORD: Record<StageState, string> = { ok: '정상', warn: '주의', fail: '문제', unknown: '확인 전' };
  const last = $derived(report?.last_generation ?? lastGeneration);
  const error = $derived(report?.error ?? null);

  function errorTitle(e: EngineError): string {
    return [categoryLabel(e.category), e.http_status ? `HTTP ${e.http_status}` : '', e.provider_code ?? ''].filter(Boolean).join(' · ');
  }

  function stageNote(key: string, state: StageState): string {
    if (state !== 'unknown') return '';
    if (key === 'generation') return '실제 생성을 해야 바뀝니다.';
    if (!report) return '';
    return '앞 단계 문제로 확인하지 못했습니다.';
  }
</script>

<section class="check" aria-live="polite" aria-busy={checking}>
  <ol class="stages">
    {#each stages as s (s.key)}
      <li class={s.state}>
        <span class="dot" aria-hidden="true">
          {#if s.state === 'ok'}<Icon name="check" size={13} />{:else if s.state === 'fail'}<Icon name="close" size={13} />{:else if s.state === 'warn'}!{/if}
        </span>
        <div class="txt">
          <div class="row">
            <span class="lbl">{s.label}</span>
            <span class="word">{checking && s.key !== 'generation' ? '확인 중…' : WORD[s.state]}</span>
          </div>
          {#if s.detail}<p class="detail">{s.detail}</p>{/if}
          {#if s.code}<p class="code" title={s.code}>{s.code}</p>{/if}
          {#if s.key === 'generation' && last}
            <p class="sub">{formatRelativeTime(last.at)}</p>
            <p class="code" title={last.model}>{last.model}</p>
          {/if}
          {#if stageNote(s.key, s.state) && !checking}<p class="sub">{stageNote(s.key, s.state)}</p>{/if}
        </div>
      </li>
    {/each}
  </ol>

  {#if !report && !checking}
    <p class="muted none">이 창에서 아직 확인하지 않았습니다. 연결 확인을 누르면 서버·키·모델 목록을 차례로 확인합니다(이미지를 만들지 않아 비용이 들지 않습니다).</p>
  {/if}

  {#if error && !checking}
    <div class="error" role="alert">
      <Icon name="alert" size={18} />
      <div>
        <strong>{errorTitle(error)}</strong>
        {#if checkHint(error.category)}<p>{checkHint(error.category)}</p>{/if}
        <p class="raw">{error.message}</p>
      </div>
    </div>
  {/if}

  {#if report && !checking && report.models_listed}
    <div class="models">
      <span class="label">이미지 모델 {report.image_models.length}개</span>
      {#if report.image_models.length === 0}
        <p class="muted">목록에 이미지 모델이 없습니다. 이 연결로는 이미지를 만들 수 없을 수 있습니다.</p>
      {:else}
        <ul>
          {#each report.image_models as id (id)}<li>{id}</li>{/each}
        </ul>
      {/if}
    </div>
  {/if}
</section>

<style>
  .check {
    display: flex;
    flex-direction: column;
    gap: var(--s4);
  }
  /* A compact stepper: 4 across when there is room, 2x2 otherwise; each box only as tall as its own text. */
  .stages {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    align-items: start;
    gap: var(--s2);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .stages li {
    display: flex;
    gap: var(--s2);
    padding: var(--s2) var(--s3);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    min-width: 0;
  }
  @media (max-width: 1279px) {
    .stages {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  .stages li.unknown {
    background: transparent;
    border-style: dashed;
  }
  .code {
    margin: 2px 0 0;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--muted);
    overflow-wrap: anywhere;
    word-break: normal;
  }
  .dot {
    flex: none;
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    font-size: 12px;
    font-weight: 700;
    color: #fff;
    box-shadow: inset 0 0 0 1.5px var(--faint);
  }
  .ok .dot {
    background: var(--ok);
    box-shadow: none;
  }
  .warn .dot {
    background: var(--warn);
    box-shadow: none;
    color: #16181d;
  }
  .fail .dot {
    background: var(--error);
    box-shadow: none;
  }
  .ok {
    border-color: rgba(34, 197, 94, 0.3) !important;
  }
  .fail {
    border-color: rgba(239, 68, 68, 0.4) !important;
  }
  .warn {
    border-color: rgba(245, 158, 11, 0.4) !important;
  }
  .txt {
    min-width: 0;
    flex: 1;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: var(--s2);
  }
  .lbl {
    font-weight: 600;
  }
  .word {
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
  }
  .detail {
    margin: 2px 0 0;
    font-size: 12px;
    color: #fca5a5;
    overflow-wrap: break-word;
  }
  .warn .detail {
    color: #fcd34d;
  }
  .sub {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--muted);
    overflow-wrap: break-word;
  }
  .error {
    display: flex;
    gap: var(--s3);
    padding: var(--s3) var(--s4);
    background: var(--error-soft);
    border: 1px solid rgba(239, 68, 68, 0.4);
    border-radius: var(--radius);
    color: #fca5a5;
  }
  .error :global(svg) {
    margin-top: 2px;
  }
  .error strong {
    color: #fecaca;
  }
  .error p {
    margin: 4px 0 0;
    color: var(--text);
  }
  .error .raw {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .none {
    margin: 0;
    font-size: 13px;
  }
  .models ul {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .models li {
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--panel-3);
    font-family: var(--mono);
    font-size: 12px;
  }
  .models p {
    margin: 0;
  }
</style>
