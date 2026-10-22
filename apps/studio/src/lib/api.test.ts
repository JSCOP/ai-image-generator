import { beforeEach, describe, expect, it, vi } from 'vitest';

const calls: { cmd: string; args: unknown }[] = [];
let reply: unknown = null;
let fail: unknown = undefined;

vi.mock('@tauri-apps/api/core', () => {
  class Channel<T> {
    onmessage: (m: T) => void = () => {};
  }
  return {
    Channel,
    invoke: vi.fn(async (cmd: string, args?: unknown) => {
      calls.push({ cmd, args });
      if (fail !== undefined) throw fail;
      return reply;
    }),
    convertFileSrc: (p: string) => `asset://localhost/${encodeURIComponent(p)}`,
  };
});

const api = await import('./api');

beforeEach(() => {
  calls.length = 0;
  reply = null;
  fail = undefined;
});

describe('api command names and arguments', () => {
  it('sends the Rust command names with camelCase argument keys', async () => {
    await api.appInfo();
    await api.getSettings();
    await api.saveSettings({ workDir: 'C:/w', galleryFolders: [], lastProfile: null, lastModel: null });
    await api.connections();
    await api.saveProfile({ id: null, name: 'a', kind: 'openai', baseUrl: null, concurrency: null, key: null, makeDefault: false });
    await api.removeProfile('p-1');
    await api.setDefaultProfile(null);
    await api.checkProfile('a');
    await api.listModels(null);
    await api.cancelGeneration('run-1');
    await api.gallery(100);
    await api.addGalleryFolder('D:/proj');
    await api.removeGalleryFolder('D:/proj');
    expect(calls.map((c) => c.cmd)).toEqual([
      'app_info',
      'get_settings',
      'save_settings',
      'connections',
      'save_profile',
      'remove_profile',
      'set_default_profile',
      'check_profile',
      'list_models',
      'cancel_generation',
      'gallery',
      'add_gallery_folder',
      'remove_gallery_folder',
    ]);
    expect(calls[2].args).toEqual({ settings: { workDir: 'C:/w', galleryFolders: [], lastProfile: null, lastModel: null } });
    expect(calls[4].args).toMatchObject({ input: { name: 'a', kind: 'openai', makeDefault: false } });
    expect(calls[5].args).toEqual({ id: 'p-1' });
    expect(calls[6].args).toEqual({ id: null });
    expect(calls[7].args).toEqual({ name: 'a' });
    expect(calls[8].args).toEqual({ name: null });
    expect(calls[9].args).toEqual({ runId: 'run-1' });
    expect(calls[10].args).toEqual({ limit: 100, peers: true });
    expect(calls[11].args).toEqual({ path: 'D:/proj' });
  });

  it('gallery sharing commands send their arguments', async () => {
    await api.gallery(undefined, false);
    await api.discoverGalleries();
    await api.shareStatus();
    await api.setShare({ enabled: true, port: 8329 });
    await api.newShareToken();
    await api.checkPeer('100.64.0.10', 't');
    await api.addPeer('', '100.64.0.10', 't');
    await api.removePeer('peer-1');
    expect(calls).toEqual([
      { cmd: 'gallery', args: { limit: null, peers: false } },
      { cmd: 'discover_galleries', args: undefined },
      { cmd: 'share_status', args: undefined },
      { cmd: 'set_share', args: { share: { enabled: true, port: 8329 } } },
      { cmd: 'new_share_token', args: undefined },
      { cmd: 'check_peer', args: { url: '100.64.0.10', token: 't' } },
      { cmd: 'add_peer', args: { name: '', url: '100.64.0.10', token: 't' } },
      { cmd: 'remove_peer', args: { id: 'peer-1' } },
    ]);
  });

  it("isRemotePath tells another PC's paths from local ones", () => {
    expect(api.isRemotePath('peer://peer-1/C:/w/a.png')).toBe(true);
    expect(api.isRemotePath('C:/w/a.png')).toBe(false);
  });

  it('inspect_reference sends the path', async () => {
    await api.inspectReference('D:/refs/cup.png');
    expect(calls).toEqual([{ cmd: 'inspect_reference', args: { path: 'D:/refs/cup.png' } }]);
  });

  it('start_generation passes the input and a channel that reaches the callback', async () => {
    const seen: unknown[] = [];
    const input = {
      profile: null,
      prompt: 'p',
      avoid: '',
      imageModel: 'gpt-image-2',
      size: '1024x1024',
      quality: 'low',
      count: 1,
      references: [],
      background: 'auto' as const,
      fit: 'contain' as const,
      topic: null,
      resume: false,
    };
    await api.startGeneration(input, (ev) => seen.push(ev));
    expect(calls[0].cmd).toBe('start_generation');
    const args = calls[0].args as { input: unknown; onEvent: { onmessage: (m: unknown) => void } };
    expect(args.input).toEqual(input);
    args.onEvent.onmessage({ event: 'imageStarted', data: { index: 1, output: 'x' } });
    expect(seen).toEqual([{ event: 'imageStarted', data: { index: 1, output: 'x' } }]);
  });
});

describe('api errors', () => {
  it('passes a ServiceError through', async () => {
    fail = { category: 'not_found', message: '폴더가 없습니다' };
    await expect(api.gallery()).rejects.toEqual({ category: 'not_found', message: '폴더가 없습니다' });
  });

  it('turns anything else into an internal ServiceError', async () => {
    fail = new TypeError("Cannot read properties of undefined (reading 'invoke')");
    await expect(api.connections()).rejects.toEqual({
      category: 'internal',
      message: "앱과 연결되지 않았습니다: Cannot read properties of undefined (reading 'invoke')",
    });
    fail = 'plain text';
    await expect(api.connections()).rejects.toEqual({ category: 'internal', message: '앱과 연결되지 않았습니다: plain text' });
  });

  it('errorText puts the category label before the message', () => {
    expect(api.errorText({ category: 'auth', message: '401 Unauthorized' })).toBe('키가 거부됨 · 401 Unauthorized');
  });
});

describe('imageSrc', () => {
  it('uses the asset protocol for local files', () => {
    expect(api.imageSrc('C:/a b.png')).toBe('asset://localhost/C%3A%2Fa%20b.png');
  });
});
