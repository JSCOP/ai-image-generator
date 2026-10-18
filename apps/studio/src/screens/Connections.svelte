<script lang="ts">
  // Placeholder until Task 6 (connections screen); already shows why connections could not be loaded.
  import Button from '../components/Button.svelte';
  import Empty from '../components/Empty.svelte';
  import { errorText } from '../lib/api';
  import { app, refreshConnections } from '../lib/state.svelte';
</script>

{#if app.connectionsError}
  <Empty icon="alert" title="연결 정보를 읽지 못했습니다" text={errorText(app.connectionsError)}>
    <Button variant="primary" icon="refresh" onclick={refreshConnections}>다시 읽기</Button>
  </Empty>
{:else}
  <Empty icon="connections" title="연결 화면" text="이 화면은 다음 단계에서 채워집니다." />
{/if}
