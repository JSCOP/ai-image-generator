<script lang="ts">
  // "다른 PC" dialog: share this PC's galleries over the private network (Tailscale) and add other PCs' shared galleries.
  // Nothing here writes or deletes images; another PC can only look.
  import { addPeer, errorText, newShareToken, removePeer, setShare, shareStatus, toServiceError } from '../lib/api';
  import { copyText } from '../lib/clipboard';
  import { loadGallery } from '../lib/gallery.svelte';
  import { app, ask, toast, toastError } from '../lib/state.svelte';
  import type { ServiceError, ShareView } from '../lib/types';
  import Button from './Button.svelte';
  import Dialog from './Dialog.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let share = $state<ShareView | null>(null);
  let shareError = $state<ServiceError | null>(null);
  let busy = $state(false);
  let showToken = $state(false);
  let form = $state({ name: '', url: '', token: '' });
  let adding = $state(false);
  let addError = $state<ServiceError | null>(null);

  const peers = $derived(app.settings?.peers ?? []);
  const address = $derived(share?.address ? `http://${share.address}:${share.port}` : null);

  $effect(() => {
    if (open) void refresh();
  });

  async function refresh() {
    try {
      share = await shareStatus();
      shareError = null;
    } catch (e) {
      shareError = toServiceError(e);
    }
  }

  async function toggle() {
    if (!share) return;
    busy = true;
    try {
      share = await setShare({ enabled: !share.enabled, port: share.port });
      if (share.enabled && share.running) toast('이 PC 공유를 켰습니다. 처음이면 Windows 방화벽 허용 창에서 "허용"을 누르세요.', 'ok', 7000);
    } catch (e) {
      toastError(toServiceError(e));
    } finally {
      busy = false;
    }
  }

  async function renewToken() {
    const ok = await ask({
      title: '새 공유 토큰',
      message: '지금 토큰으로 연결한 다른 PC는 새 토큰을 다시 넣을 때까지 이 PC의 이미지를 볼 수 없습니다.',
      confirmText: '새 토큰 만들기',
    });
    if (!ok) return;
    busy = true;
    try {
      share = await newShareToken();
      showToken = true;
    } catch (e) {
      toastError(toServiceError(e));
    } finally {
      busy = false;
    }
  }

  async function copy(text: string, what: string) {
    if (await copyText(text)) toast(`${what}를 복사했습니다.`, 'ok', 2500);
  }

  async function add() {
    adding = true;
    addError = null;
    try {
      app.settings = await addPeer(form.name, form.url, form.token);
      form = { name: '', url: '', token: '' };
      toast('다른 PC를 추가했습니다. 그 PC의 이미지가 갤러리에 함께 나옵니다.', 'ok');
      void loadGallery();
    } catch (e) {
      addError = toServiceError(e);
    } finally {
      adding = false;
    }
  }

  async function forget(id: string, name: string) {
    const ok = await ask({
      title: '다른 PC 빼기',
      message: `"${name}"의 이미지를 갤러리에서 뺍니다. 그 PC의 파일은 그대로이고, 이 PC에 저장한 미리보기만 지웁니다.`,
      confirmText: '빼기',
    });
    if (!ok) return;
    try {
      app.settings = await removePeer(id);
      void loadGallery();
    } catch (e) {
      toastError(toServiceError(e));
    }
  }
</script>

<Dialog {open} title="다른 PC와 갤러리 함께 보기" {onclose} width={560}>
  <section>
    <h3>이 PC 공유</h3>
    <p class="muted">켜 두면 같은 Tailscale 사설망의 다른 PC가 이 PC의 갤러리를 볼 수 있습니다. 보기만 할 수 있고 파일을 바꾸거나 지울 수는 없습니다.</p>
    {#if shareError}
      <p class="err">{errorText(shareError)}</p>
    {:else if share}
      <div class="row">
        <span class="state" class:on={share.running}>{share.running ? '공유 중' : '꺼짐'}</span>
        <Button size="sm" variant={share.enabled ? 'secondary' : 'primary'} disabled={busy} onclick={toggle}>{share.enabled ? '끄기' : '켜기'}</Button>
      </div>
      {#if share.error}<p class="err">{share.error}</p>{/if}
      {#if share.enabled}
        <dl>
          <dt>이 PC</dt>
          <dd>{share.name}</dd>
          <dt>주소</dt>
          <dd>
            {#if address}
              <code>{address}</code>
              <Button size="sm" variant="ghost" icon="copy" label="주소 복사" title="주소 복사" onclick={() => address && copy(address, '주소')} />
            {:else}
              <span class="warn">Tailscale에 연결되어 있지 않습니다. Tailscale에 로그인하면 주소가 나타납니다.</span>
            {/if}
          </dd>
          <dt>토큰</dt>
          <dd>
            <code class="token">{showToken ? share.token : '•'.repeat(16)}</code>
            <Button size="sm" variant="ghost" onclick={() => (showToken = !showToken)}>{showToken ? '가리기' : '보기'}</Button>
            <Button size="sm" variant="ghost" icon="copy" label="토큰 복사" title="토큰 복사" onclick={() => share && copy(share.token, '토큰')} />
            <Button size="sm" variant="ghost" icon="refresh" label="새 토큰" title="새 토큰 만들기" disabled={busy} onclick={renewToken} />
          </dd>
        </dl>
        <p class="muted small">다른 PC의 Image Studio에서 위 주소와 토큰을 넣으면 됩니다. 토큰은 비밀번호처럼 다루세요.</p>
      {/if}
    {:else}
      <p class="muted">불러오는 중…</p>
    {/if}
  </section>

  <section>
    <h3>다른 PC의 갤러리</h3>
    {#if peers.length === 0}
      <p class="muted">아직 없습니다. 다른 PC에서 "이 PC 공유"를 켠 뒤 그 주소와 토큰을 넣으세요.</p>
    {:else}
      <ul class="peers">
        {#each peers as peer (peer.id)}
          <li>
            <span class="name">{peer.name}</span>
            <code>{peer.url}</code>
            <Button size="sm" variant="ghost" icon="close" label="{peer.name} 빼기" title="빼기" onclick={() => forget(peer.id, peer.name)} />
          </li>
        {/each}
      </ul>
    {/if}
    <form
      class="add"
      onsubmit={(e) => {
        e.preventDefault();
        void add();
      }}
    >
      <input class="field" autocomplete="off" placeholder="주소 (예: 100.x.y.z 또는 그 PC의 Tailscale 이름)" bind:value={form.url} aria-label="다른 PC 주소" />
      <input class="field" autocomplete="off" type="password" placeholder="그 PC의 공유 토큰" bind:value={form.token} aria-label="공유 토큰" />
      <input class="field" autocomplete="off" placeholder="이름 (비우면 그 PC 이름)" bind:value={form.name} aria-label="이름" />
      <Button type="submit" variant="primary" icon="plus" disabled={adding || !form.url.trim() || !form.token.trim()}>{adding ? '확인 중…' : '연결 확인 후 추가'}</Button>
      {#if addError}<p class="err">{errorText(addError)}</p>{/if}
    </form>
  </section>
</Dialog>

<style>
  section + section {
    margin-top: var(--s4);
    padding-top: var(--s4);
    border-top: 1px solid var(--border);
  }
  h3 {
    margin: 0 0 var(--s1);
    font-size: 14px;
  }
  p {
    margin: 0 0 var(--s2);
    font-size: 13px;
  }
  .small {
    font-size: 12px;
  }
  .err {
    color: #fecaca;
  }
  .warn {
    color: #fcd34d;
    font-size: 13px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin-bottom: var(--s2);
  }
  .state {
    font-size: 13px;
    color: var(--muted);
  }
  .state.on {
    color: var(--ok, #86efac);
  }
  dl {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr);
    gap: 6px var(--s2);
    margin: 0 0 var(--s2);
    font-size: 13px;
    align-items: center;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: var(--s1);
    min-width: 0;
    flex-wrap: wrap;
  }
  code {
    font-family: var(--mono);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .token {
    max-width: 260px;
  }
  .peers {
    list-style: none;
    margin: 0 0 var(--s3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s1);
  }
  .peers li {
    display: flex;
    align-items: center;
    gap: var(--s2);
    font-size: 13px;
  }
  .peers .name {
    font-weight: 600;
  }
  .peers code {
    color: var(--muted);
    flex: 1;
  }
  .add {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
  }
</style>
