<script lang="ts">
  // App shell: icon navigation, top bar, current screen, toasts and the confirm dialog.
  import Button from './components/Button.svelte';
  import ConfirmHost from './components/ConfirmHost.svelte';
  import Empty from './components/Empty.svelte';
  import NavRail from './components/NavRail.svelte';
  import Toast from './components/Toast.svelte';
  import TopBar from './components/TopBar.svelte';
  import { errorText } from './lib/api';
  import { app, loadApp, navigate } from './lib/state.svelte';
  import Connections from './screens/Connections.svelte';
  import Gallery from './screens/Gallery.svelte';
  import Generate from './screens/Generate.svelte';

  loadApp();
</script>

<div class="shell">
  <NavRail screen={app.screen} onnavigate={navigate} />
  <div class="main">
    <TopBar />
    <main>
      {#if app.loading}
        <div class="loading" aria-busy="true">불러오는 중…</div>
      {:else if app.loadError}
        <Empty icon="alert" title="앱 정보를 불러오지 못했습니다" text={errorText(app.loadError)}>
          <Button variant="primary" icon="refresh" onclick={loadApp}>다시 시도</Button>
        </Empty>
      {:else if app.screen === 'generate'}
        <Generate />
      {:else if app.screen === 'gallery'}
        <Gallery />
      {:else}
        <Connections />
      {/if}
    </main>
  </div>
</div>

<Toast />
<ConfirmHost />

<style>
  .shell {
    height: 100%;
    display: grid;
    grid-template-columns: var(--nav-w) minmax(0, 1fr);
    min-width: 1100px;
    min-height: 700px;
  }
  .main {
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: var(--top-h) minmax(0, 1fr);
  }
  main {
    min-height: 0;
    overflow: hidden;
  }
  .loading {
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--muted);
  }
</style>
