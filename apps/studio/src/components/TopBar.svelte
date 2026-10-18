<script lang="ts">
  // Top bar: app mark and name, current screen, active connection badge (name, kind, 4 stage dots).
  import { activeConnection, stagesOf } from '../lib/connection';
  import { app, navigate } from '../lib/state.svelte';
  import StageBadges from './StageBadges.svelte';

  const TITLES = { generate: '생성', gallery: '갤러리', connections: '연결' } as const;

  const active = $derived(activeConnection(app.connections));
  const stages = $derived(
    active ? stagesOf(app.checks[active.profile ? active.profile.name : 'env'] ?? null, active.profile?.lastGeneration ?? null) : [],
  );
</script>

<header>
  <div class="brand">
    <span class="mark" aria-hidden="true">
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="#fff" stroke-width="2.2" stroke-linejoin="round">
        <rect x="4" y="4" width="16" height="16" rx="2.5" />
        <path d="M15.5 7.5l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z" fill="#fff" stroke-width="1" />
      </svg>
    </span>
    <span class="name">Image Studio</span>
    <span class="sep" aria-hidden="true">/</span>
    <span class="where">{TITLES[app.screen]}</span>
  </div>

  <button type="button" class="conn" class:none={!active} onclick={() => navigate('connections')} title="연결 화면 열기">
    {#if active}
      <StageBadges {stages} compact />
      <span class="cname ellipsis">{active.name}</span>
      <span class="kind">{active.kindLabel}</span>
    {:else if app.connections}
      <span class="warn-dot" aria-hidden="true"></span>
      <span>연결 없음 · 추가하기</span>
    {:else}
      <span class="muted">연결 불러오는 중…</span>
    {/if}
  </button>
</header>

<style>
  header {
    height: var(--top-h);
    display: flex;
    align-items: center;
    gap: var(--s4);
    padding: 0 var(--s4) 0 var(--s4);
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }
  .brand {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--s3);
  }
  .mark {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: linear-gradient(135deg, #7c4dff, #8b5cf6);
  }
  .name {
    font-size: 16px;
    font-weight: 600;
  }
  .sep {
    color: var(--faint);
  }
  .where {
    color: var(--muted);
  }
  .conn {
    max-width: 360px;
    height: 34px;
    display: inline-flex;
    align-items: center;
    gap: var(--s2);
    padding: 0 var(--s3);
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--panel);
    cursor: pointer;
  }
  .conn:hover {
    border-color: var(--border-strong);
  }
  .conn.none {
    border-color: rgba(245, 158, 11, 0.45);
    color: #fcd34d;
  }
  .cname {
    font-weight: 600;
  }
  .kind {
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
  }
  .warn-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warn);
  }
</style>
