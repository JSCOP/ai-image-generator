// Unit tests of the connections screen form logic (src/lib/connectionsForm.ts); the screen itself only wires these to inputs.
import { describe, expect, it } from 'vitest';
import {
  changeKind,
  checkHint,
  formFromProfile,
  isDirty,
  keyModes,
  newForm,
  officialUrl,
  toInput,
  transportWarning,
  validate,
  type ConnectionForm,
} from '../lib/connectionsForm';
import type { ProfileView } from '../lib/types';

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

function form(over: Partial<ConnectionForm> = {}): ConnectionForm {
  return { ...newForm(), name: '새 연결', baseUrl: 'http://127.0.0.1:8317/v1', ...over };
}

describe('kind change fills the address', () => {
  it('official kinds get their address, locked', () => {
    const f = changeKind(newForm(), 'openai');
    expect(f.baseUrl).toBe('https://api.openai.com/v1');
    expect(f.urlUnlocked).toBe(false);
    expect(changeKind(f, 'gemini').baseUrl).toBe('https://generativelanguage.googleapis.com');
    expect(changeKind(f, 'xai').baseUrl).toBe('https://api.x.ai/v1');
  });

  it('switching from an official kind to a proxy clears the official address', () => {
    const f = changeKind(changeKind(newForm(), 'openai'), 'cliproxy');
    expect(f.baseUrl).toBe('');
    expect(officialUrl('cliproxy')).toBeNull();
    expect(officialUrl('openai_compatible')).toBeNull();
  });

  it('an address the user typed for a proxy is kept between proxy kinds', () => {
    const typed = { ...newForm(), baseUrl: 'http://100.64.0.10:8317/v1' };
    expect(changeKind(typed, 'openai_compatible').baseUrl).toBe('http://100.64.0.10:8317/v1');
  });

  it('an unlocked official address is replaced and locked again on another official kind', () => {
    const f = { ...changeKind(newForm(), 'openai'), baseUrl: 'https://gateway.example.com/v1', urlUnlocked: true };
    const g = changeKind(f, 'xai');
    expect(g.baseUrl).toBe('https://api.x.ai/v1');
    expect(g.urlUnlocked).toBe(false);
  });

  it('the kind of a saved profile cannot change', () => {
    const f = formFromProfile(profile());
    expect(changeKind(f, 'openai')).toEqual(f);
  });

  it('a saved official profile with its own address opens unlocked', () => {
    const f = formFromProfile(profile({ kind: 'openai', baseUrl: 'https://gateway.example.com/v1' }));
    expect(f.urlUnlocked).toBe(true);
    const g = formFromProfile(profile({ kind: 'openai', baseUrl: 'https://api.openai.com/v1' }));
    expect(g.urlUnlocked).toBe(false);
  });
});

describe('validate', () => {
  it('a blank or space-only name is refused', () => {
    expect(validate(form({ name: '' }), []).name).toBe('이름을 입력하세요.');
    expect(validate(form({ name: '   \t' }), []).name).toBe('이름을 입력하세요.');
    expect(validate(form({ name: '  회사 ' }), []).name).toBeUndefined();
  });

  it('names: too long, id-like, duplicate (case-insensitive, not itself)', () => {
    expect(validate(form({ name: 'ㄱ'.repeat(65) }), []).name).toContain('64');
    expect(validate(form({ name: 'ㄱ'.repeat(64) }), []).name).toBeUndefined();
    expect(validate(form({ name: 'p-12ab34cd' }), []).name).toBeDefined();
    expect(validate(form({ name: 'LOCAL' }), [profile()]).name).toBe('같은 이름의 연결이 이미 있습니다.');
    expect(validate({ ...formFromProfile(profile()), name: 'Local' }, [profile()]).name).toBeUndefined();
  });

  it('a proxy needs an address; an official kind does not', () => {
    expect(validate(form({ baseUrl: ' ' }), []).baseUrl).toBe('주소를 입력하세요.');
    const official = { ...changeKind(newForm(), 'openai'), name: 'o', key: 'sk-test' };
    expect(validate(official, []).baseUrl).toBeUndefined();
  });

  it('addresses: scheme, credentials, query, Gemini API path', () => {
    expect(validate(form({ baseUrl: 'ftp://x' }), []).baseUrl).toBeDefined();
    expect(validate(form({ baseUrl: '127.0.0.1:8317' }), []).baseUrl).toBeDefined();
    expect(validate(form({ baseUrl: 'http://u:p@host/v1' }), []).baseUrl).toBeDefined();
    expect(validate(form({ baseUrl: 'https://host/v1?x=1' }), []).baseUrl).toBeDefined();
    expect(validate(form({ baseUrl: 'https://host/v 1' }), []).baseUrl).toBeDefined();
    const gemini = { ...changeKind(newForm(), 'gemini'), name: 'g', key: 'k', urlUnlocked: true };
    expect(validate({ ...gemini, baseUrl: 'https://generativelanguage.googleapis.com/v1beta' }, []).baseUrl).toContain('v1beta');
  });

  it('concurrency: blank = kind default, else 1..=32 whole number', () => {
    expect(validate(form({ concurrency: '' }), []).concurrency).toBeUndefined();
    expect(validate(form({ concurrency: '32' }), []).concurrency).toBeUndefined();
    expect(validate(form({ concurrency: '0' }), []).concurrency).toBeDefined();
    expect(validate(form({ concurrency: '33' }), []).concurrency).toBeDefined();
    expect(validate(form({ concurrency: '2.5' }), []).concurrency).toBeDefined();
    expect(validate(form({ concurrency: 'abc' }), []).concurrency).toBeDefined();
  });

  it('key: a new official connection needs one; "replace" needs a value', () => {
    const official = { ...changeKind(newForm(), 'openai'), name: 'o' };
    expect(validate(official, []).key).toBeDefined();
    expect(validate({ ...official, key: '  ' }, []).key).toBeDefined();
    expect(validate({ ...official, key: 'sk-test' }, []).key).toBeUndefined();
    expect(validate(form({ key: '' }), []).key).toBeUndefined(); // a proxy may have no key
    const edit = { ...formFromProfile(profile()), keyMode: 'replace' as const, key: ' ' };
    expect(validate(edit, [profile()]).key).toBe('새 키를 입력하세요.');
  });
});

describe('toInput key value', () => {
  const saved = profile();

  it('keep -> null (stored key untouched)', () => {
    const f = formFromProfile(saved);
    expect(f.keyMode).toBe('keep');
    expect(toInput(f).key).toBeNull();
    // A value typed before switching back to "keep" is not sent.
    expect(toInput({ ...f, key: 'typed' }).key).toBeNull();
  });

  it('replace -> the new key, trimmed', () => {
    expect(toInput({ ...formFromProfile(saved), keyMode: 'replace', key: '  sk-new  ' }).key).toBe('sk-new');
  });

  it('clear -> "" (delete the stored key)', () => {
    expect(toInput({ ...formFromProfile(saved), keyMode: 'clear', key: 'ignored' }).key).toBe('');
  });

  it('new profile: blank -> null, else the key', () => {
    expect(toInput(form({ key: '' })).key).toBeNull();
    expect(toInput(form({ key: ' abc ' })).key).toBe('abc');
  });

  it('key modes offered follow whether a key is stored', () => {
    expect(keyModes(formFromProfile(saved)).map((m) => [m.value, !!m.disabled])).toEqual([
      ['keep', false],
      ['replace', false],
      ['clear', false],
    ]);
    const none = formFromProfile(profile({ keyConfigured: false }));
    expect(keyModes(none).find((m) => m.value === 'clear')?.disabled).toBe(true);
    expect(keyModes(none).find((m) => m.value === 'keep')?.label).toBe('키 없이 두기');
  });
});

describe('toInput other fields', () => {
  it('name trimmed, concurrency parsed, id kept', () => {
    const input = toInput({ ...formFromProfile(profile({ concurrency: 8 })), name: '  local 2 ' });
    expect(input).toMatchObject({ id: 'p-0000aaaa', name: 'local 2', kind: 'cliproxy', concurrency: 8, makeDefault: false });
    expect(toInput(form({ concurrency: ' ' })).concurrency).toBeNull();
    expect(toInput(form({ concurrency: '12' })).concurrency).toBe(12);
  });

  it('a locked official address is sent as null (= official address); an unlocked one as typed', () => {
    const official = { ...changeKind(newForm(), 'openai'), name: 'o', key: 'k' };
    expect(toInput(official).baseUrl).toBeNull();
    expect(toInput({ ...official, urlUnlocked: true, baseUrl: ' https://gw.example.com/v1 ' }).baseUrl).toBe('https://gw.example.com/v1');
    expect(toInput(form({ baseUrl: ' http://127.0.0.1:8317 ' })).baseUrl).toBe('http://127.0.0.1:8317');
  });

  it('a new form is never default unless asked', () => {
    expect(toInput(form()).makeDefault).toBe(false);
    expect(toInput(form({ makeDefault: true })).makeDefault).toBe(true);
  });
});

describe('isDirty', () => {
  it('compares with the saved profile', () => {
    const saved = profile();
    const f = formFromProfile(saved);
    expect(isDirty(f, saved)).toBe(false);
    expect(isDirty({ ...f, name: 'other' }, saved)).toBe(true);
    expect(isDirty({ ...f, keyMode: 'replace', key: 'x' }, saved)).toBe(true);
    expect(isDirty({ ...f, keyMode: 'replace', key: '' }, saved)).toBe(false);
    expect(isDirty({ ...f, concurrency: '4' }, saved)).toBe(true);
  });

  it('a new form is dirty once anything is typed', () => {
    expect(isDirty(newForm(), null)).toBe(false);
    expect(isDirty({ ...newForm(), name: 'a' }, null)).toBe(true);
    expect(isDirty({ ...newForm(), key: 'k' }, null)).toBe(true);
  });
});

describe('transportWarning (mirror of image_core::profiles::transport_warning)', () => {
  it('trusts https, loopback, private, Tailscale and single-label hosts', () => {
    for (const url of [
      'https://api.openai.com/v1',
      'http://127.0.0.1:8317/v1',
      'http://localhost:8317',
      'http://my.localhost',
      'http://[::1]:8317',
      'http://10.0.0.5',
      'http://172.16.0.1',
      'http://172.31.255.1',
      'http://192.168.1.10',
      'http://100.64.0.10:8317/v1',
      'http://desktop-sb818kq:8317',
      'http://box.tail1234.ts.net:8317',
      'http://[fd7a:115c:a1e0::1]:8317',
    ]) {
      expect(transportWarning(url), url).toBe(false);
    }
  });

  it('warns for plain http to a public address', () => {
    for (const url of ['http://example.com/v1', 'http://8.8.8.8', 'http://172.32.0.1', 'http://100.128.0.1', 'http://[2001:db8::1]']) {
      expect(transportWarning(url), url).toBe(true);
    }
  });

  it('unparseable text gives no warning (validation reports it)', () => {
    expect(transportWarning('not a url')).toBe(false);
    expect(transportWarning('')).toBe(false);
  });
});

describe('checkHint (spec §6.2 causes)', () => {
  it('explains the connection failures in plain words', () => {
    expect(checkHint('dns')).toContain('Tailscale');
    expect(checkHint('network')).toContain('포트');
    expect(checkHint('timeout')).toContain('방화벽');
    expect(checkHint('auth')).toContain('키');
    expect(checkHint('not_found')).toContain('/v1');
    expect(checkHint('redirect')).toContain('따라가지 않');
    expect(checkHint('quota')).toContain('한도');
  });

  it('gives nothing for categories without a known cause', () => {
    expect(checkHint('internal')).toBe('');
  });
});
