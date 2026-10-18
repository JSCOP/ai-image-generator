<script lang="ts">
  // Four connection stages (server, key, models, real generation) as dots, with labels unless compact.
  import type { Stage, StageState } from '../lib/connection';

  interface Props {
    stages: Stage[];
    compact?: boolean;
  }

  let { stages, compact = false }: Props = $props();

  const WORD: Record<StageState, string> = { ok: '정상', warn: '주의', fail: '문제', unknown: '확인 전' };

  function tip(s: Stage): string {
    const why = [s.detail, s.code ? `(${s.code})` : ''].filter(Boolean).join(' ');
    return why ? `${s.label}: ${WORD[s.state]} — ${why}` : `${s.label}: ${WORD[s.state]}`;
  }
</script>

<ul class="stages" class:compact aria-label="연결 상태">
  {#each stages as s (s.key)}
    <li title={tip(s)}>
      <span class="dot {s.state}" aria-hidden="true"></span>
      {#if compact}<span class="visually-hidden">{tip(s)}</span>{:else}<span class="lbl">{s.label}</span>{/if}
    </li>
  {/each}
</ul>

<style>
  .stages {
    display: flex;
    align-items: center;
    gap: var(--s3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .compact {
    gap: 5px;
  }
  li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border-strong);
  }
  .ok {
    background: var(--ok);
  }
  .warn {
    background: var(--warn);
  }
  .fail {
    background: var(--error);
  }
  .unknown {
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--faint);
  }
</style>
