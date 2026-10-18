import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConnectionsView, ServiceError } from './types';

const info = { version: '0.1.0', home: 'C:/h', workDir: 'C:/w' };
const settings = { workDir: 'C:/w', galleryFolders: [], lastProfile: null, lastModel: null };
const broken: ServiceError = { category: 'invalid_request', message: 'profiles.json을 읽을 수 없습니다' };
const view = { profiles: [{ name: 'local' }], defaultProfile: 'local', env: null } as unknown as ConnectionsView;

const mocks = {
  appInfo: vi.fn(async (): Promise<unknown> => info),
  getSettings: vi.fn(async (): Promise<unknown> => settings),
  connections: vi.fn(async (): Promise<unknown> => view),
};

vi.mock('./api', () => ({
  appInfo: () => mocks.appInfo(),
  getSettings: () => mocks.getSettings(),
  connections: () => mocks.connections(),
  toServiceError: (e: unknown) => e as ServiceError,
  errorText: (e: ServiceError) => e.message,
}));
vi.mock('./connection', () => ({ firstScreen: () => 'generate' }));

const state = await import('./state.svelte');

beforeEach(() => {
  mocks.appInfo.mockImplementation(async () => info);
  mocks.getSettings.mockImplementation(async () => settings);
  mocks.connections.mockImplementation(async () => view);
  state.app.screen = 'generate';
  state.app.connections = null;
  state.app.connectionsError = null;
  state.app.loadError = null;
});

describe('loadApp', () => {
  it('keeps the app usable when only connections fail', async () => {
    mocks.connections.mockImplementation(async () => {
      throw broken;
    });
    state.app.screen = 'gallery';
    await state.loadApp();
    expect(state.app.loadError).toBeNull();
    expect(state.app.loading).toBe(false);
    expect(state.app.info).toEqual(info);
    expect(state.app.settings).toEqual(settings);
    expect(state.app.connections).toBeNull();
    expect(state.app.connectionsError).toEqual(broken);
    expect(state.app.screen).toBe('gallery');
  });

  it('shows the full-screen error when settings fail', async () => {
    mocks.getSettings.mockImplementation(async () => {
      throw broken;
    });
    await state.loadApp();
    expect(state.app.loadError).toEqual(broken);
  });

  it('clears the connections error once connections load again', async () => {
    mocks.connections.mockImplementation(async () => {
      throw broken;
    });
    await state.loadApp();
    mocks.connections.mockImplementation(async () => view);
    await state.refreshConnections();
    expect(state.app.connectionsError).toBeNull();
    expect(state.app.connections).toEqual(view);
  });
});
