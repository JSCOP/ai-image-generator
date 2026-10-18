import { describe, expect, it } from 'vitest';
import { activeConnection, firstScreen, stagesOf } from './connection';
import type { ConnectionReport, ConnectionsView, ProfileView } from './types';

function profile(over: Partial<ProfileView> = {}): ProfileView {
  return {
    id: 'p-0000aaaa',
    name: 'local',
    kind: 'cliproxy',
    kindLabel: 'CLIProxyAPI',
    baseUrl: 'http://127.0.0.1:8317/v1',
    concurrency: null,
    isDefault: true,
    keyConfigured: true,
    warning: null,
    lastGeneration: null,
    ...over,
  };
}

function report(over: Partial<ConnectionReport> = {}): ConnectionReport {
  return {
    profile: 'local',
    kind: 'cliproxy',
    base_url: 'http://127.0.0.1:8317/v1',
    key_configured: true,
    reachable: true,
    authenticated: true,
    models_listed: true,
    image_models: ['gpt-image-2'],
    last_generation: null,
    warning: null,
    error: null,
    ...over,
  };
}

const states = (r?: ConnectionReport, p?: ProfileView) => stagesOf(r ?? null, p?.lastGeneration ?? null).map((s) => s.state);

describe('stagesOf', () => {
  it('is unknown everywhere before any check', () => {
    expect(states()).toEqual(['unknown', 'unknown', 'unknown', 'unknown']);
    expect(stagesOf(null, null).map((s) => s.label)).toEqual(['서버', '인증', '모델', '생성']);
  });

  it('shows a fully working connection, generation from the last real run', () => {
    expect(states(report())).toEqual(['ok', 'ok', 'ok', 'unknown']);
    const last = { ok: true, at: 1, model: 'gpt-image-2', category: null, provider_code: null };
    expect(states(report({ last_generation: last }))).toEqual(['ok', 'ok', 'ok', 'ok']);
  });

  it('stops at the first broken stage and leaves later ones unknown', () => {
    const dns = report({
      reachable: false,
      authenticated: false,
      models_listed: false,
      image_models: [],
      error: { category: 'dns', provider_code: null, http_status: null, retryable: true, message: 'dns error' },
    });
    const stages = stagesOf(dns, null);
    expect(stages.map((s) => s.state)).toEqual(['fail', 'unknown', 'unknown', 'unknown']);
    expect(stages[0].detail).toBe('주소를 찾을 수 없음(오타 또는 Tailscale 꺼짐)');

    const auth = report({ authenticated: false, models_listed: false, image_models: [] });
    expect(states(auth)).toEqual(['ok', 'fail', 'unknown', 'unknown']);
  });

  it('warns when the list has no image model', () => {
    expect(states(report({ image_models: [] }))).toEqual(['ok', 'ok', 'warn', 'unknown']);
  });

  it('a failed last generation shows the reason (e.g. spending limit)', () => {
    const p = profile({ lastGeneration: { ok: false, at: 1, model: 'grok-imagine-image', category: 'quota', provider_code: 'spending_limit' } });
    const stages = stagesOf(null, p.lastGeneration);
    expect(stages[3]).toMatchObject({ state: 'fail', detail: '사용 한도 초과 (spending_limit)' });
  });
});

describe('activeConnection / firstScreen', () => {
  const view = (over: Partial<ConnectionsView> = {}): ConnectionsView => ({ profiles: [profile()], env: null, active: 'local', ...over });

  it('names the active profile with its kind', () => {
    expect(activeConnection(view())).toMatchObject({ name: 'local', kindLabel: 'CLIProxyAPI', profile: { id: 'p-0000aaaa' } });
  });

  it('names the environment connection', () => {
    const env = { baseUrl: 'http://100.64.0.1:8317/v1', source: 'user', keyConfigured: true };
    expect(activeConnection(view({ env, active: 'env' }))).toMatchObject({ name: '환경변수 연결', kindLabel: 'CLIProxyAPI', profile: null });
  });

  it('is null when nothing is active', () => {
    expect(activeConnection(view({ profiles: [], active: null }))).toBeNull();
    expect(activeConnection(null)).toBeNull();
  });

  it('opens the connections screen first when there is no connection at all', () => {
    expect(firstScreen(view({ profiles: [], env: null, active: null }))).toBe('connections');
    expect(firstScreen(view())).toBe('generate');
    expect(firstScreen(null)).toBe('generate');
  });
});
