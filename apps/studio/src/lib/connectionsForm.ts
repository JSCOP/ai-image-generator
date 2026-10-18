// Connection edit form (pure): kind/address rules, checks before saving, and the ProfileInput sent to save_profile.
// The Rust service checks everything again; these rules only give instant feedback and never touch a stored key.

import type { Category, ProfileInput, ProfileView, ProviderKind } from './types';

const CHECK_HINTS: Partial<Record<Category, string>> = {
  dns: '주소에 오타가 있거나 Tailscale이 꺼져 있거나 로그아웃 상태입니다.',
  network: '서버가 꺼져 있거나 포트 번호가 틀렸습니다.',
  timeout: '방화벽이 막았거나 Tailscale 경로 문제, 또는 서버가 응답하지 않습니다.',
  tls: '서버 인증서를 믿을 수 없습니다. 주소의 https/http를 확인하세요.',
  auth: '키가 틀렸거나 만료되었습니다. 키를 바꿔 저장한 뒤 다시 확인하세요.',
  permission: '이 키에 권한이 없습니다. 공급자 코드를 확인하세요.',
  quota: '지출 한도나 사용 한도에 걸렸습니다. 공급자 콘솔에서 한도를 확인하세요.',
  rate_limit: '요청이 너무 많습니다. 잠시 뒤 다시 확인하세요.',
  not_found: '주소 경로가 틀렸습니다. 주소 끝의 /v1이 맞는지 확인하세요.',
  redirect: '서버가 다른 주소로 보내려 합니다. 키를 지키려고 따라가지 않았습니다. 최종 주소를 직접 입력하세요.',
};

/** Likely cause of a failed connection check (spec §6.2 table), or "". */
export function checkHint(category: Category): string {
  return CHECK_HINTS[category] ?? '';
}

export type KeyMode = 'keep' | 'replace' | 'clear';

export interface ConnectionForm {
  /** null = new connection. */
  id: string | null;
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  /** Official kinds lock their address until the user unlocks it. */
  urlUnlocked: boolean;
  /** Text of the concurrency field; blank = the kind's default. */
  concurrency: string;
  /** Saved connections only; a new connection sends `key` when it is not blank. */
  keyMode: KeyMode;
  /** Typed key. Lives only in the form and is cleared after saving. */
  key: string;
  /** Whether the saved connection has a stored key. */
  keyConfigured: boolean;
  makeDefault: boolean;
}

export interface KindInfo {
  kind: ProviderKind;
  /** Same text as image_core ProviderKind::label. */
  label: string;
  hint: string;
  officialUrl: string | null;
  keyRequired: boolean;
  defaultConcurrency: number;
}

export const KINDS: KindInfo[] = [
  {
    kind: 'cliproxy',
    label: 'CLIProxyAPI',
    hint: '이 PC나 Tailscale로 연결한 CLIProxyAPI 서버. 모든 모델을 한 주소로 씁니다.',
    officialUrl: null,
    keyRequired: false,
    defaultConcurrency: 30,
  },
  {
    kind: 'openai',
    label: 'OpenAI 공식',
    hint: 'OpenAI API 키로 직접 연결합니다(gpt-image 모델).',
    officialUrl: 'https://api.openai.com/v1',
    keyRequired: true,
    defaultConcurrency: 4,
  },
  {
    kind: 'gemini',
    label: 'Gemini 공식',
    hint: 'Google AI Studio 키로 직접 연결합니다(gemini image 모델).',
    officialUrl: 'https://generativelanguage.googleapis.com',
    keyRequired: true,
    defaultConcurrency: 4,
  },
  {
    kind: 'xai',
    label: 'xAI 공식',
    hint: 'xAI 키로 직접 연결합니다(grok-imagine 모델).',
    officialUrl: 'https://api.x.ai/v1',
    keyRequired: true,
    defaultConcurrency: 4,
  },
  {
    kind: 'openai_compatible',
    label: 'OpenAI 호환(기타)',
    hint: 'OpenAI 형식을 따르는 다른 서버. 주소를 직접 입력합니다.',
    officialUrl: null,
    keyRequired: false,
    defaultConcurrency: 30,
  },
];

export const MAX_CONCURRENCY = 32;
const MAX_NAME_CHARS = 64;

export function kindInfo(kind: ProviderKind): KindInfo {
  return KINDS.find((k) => k.kind === kind) ?? KINDS[0];
}

/** Address of an official kind, null for proxies (the user types it). */
export function officialUrl(kind: ProviderKind): string | null {
  return kindInfo(kind).officialUrl;
}

const OFFICIAL_URLS = new Set(KINDS.map((k) => k.officialUrl).filter((u): u is string => !!u));

export function newForm(kind: ProviderKind = 'cliproxy'): ConnectionForm {
  return {
    id: null,
    name: '',
    kind,
    baseUrl: officialUrl(kind) ?? '',
    urlUnlocked: false,
    concurrency: '',
    keyMode: 'replace',
    key: '',
    keyConfigured: false,
    makeDefault: false,
  };
}

export function formFromProfile(p: ProfileView): ConnectionForm {
  const official = officialUrl(p.kind);
  return {
    id: p.id,
    name: p.name,
    kind: p.kind,
    baseUrl: p.baseUrl,
    urlUnlocked: official !== null && sameUrl(p.baseUrl, official) === false,
    concurrency: p.concurrency === null ? '' : String(p.concurrency),
    keyMode: 'keep',
    key: '',
    keyConfigured: p.keyConfigured,
    makeDefault: false,
  };
}

function sameUrl(a: string, b: string): boolean {
  const clean = (u: string) => u.trim().replace(/\/+$/, '').toLowerCase();
  return clean(a) === clean(b);
}

/**
 * New connections only (a saved connection keeps its kind, as the service requires).
 * Official kinds fill in and lock their address; a proxy keeps what the user typed but drops an official address.
 */
export function changeKind(form: ConnectionForm, kind: ProviderKind): ConnectionForm {
  if (form.id !== null || form.kind === kind) return form;
  const official = officialUrl(kind);
  if (official) return { ...form, kind, baseUrl: official, urlUnlocked: false };
  const typed = form.baseUrl.trim();
  const keep = typed !== '' && ![...OFFICIAL_URLS].some((u) => sameUrl(u, typed));
  return { ...form, kind, baseUrl: keep ? form.baseUrl : '', urlUnlocked: false };
}

/** The address field is read-only (official address, not unlocked). */
export function urlLocked(form: ConnectionForm): boolean {
  return officialUrl(form.kind) !== null && !form.urlUnlocked;
}

export interface KeyModeOption {
  value: KeyMode;
  label: string;
  disabled?: boolean;
  title?: string;
}

/** Choices for a saved connection's key (Segmented options). */
export function keyModes(form: ConnectionForm): KeyModeOption[] {
  const has = form.keyConfigured;
  return [
    { value: 'keep', label: has ? '저장된 키 유지' : '키 없이 두기' },
    { value: 'replace', label: has ? '바꾸기' : '새 키 입력' },
    { value: 'clear', label: '지우기', disabled: !has, title: has ? undefined : '저장된 키가 없습니다' },
  ];
}

export interface FormErrors {
  name?: string;
  baseUrl?: string;
  concurrency?: string;
  key?: string;
}

/** Korean reason the address cannot be used, or undefined. Same rules as image_core::connection::validate_base_url. */
export function addressError(raw: string, kind: ProviderKind): string | undefined {
  const text = raw.trim();
  if (!text) return '주소를 입력하세요.';
  if (/[\u0000- \u007f]/.test(text)) return '주소에 공백을 넣을 수 없습니다.';
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return 'http:// 또는 https://로 시작하는 주소를 입력하세요(예: http://127.0.0.1:8317/v1).';
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return '주소는 http:// 또는 https://만 쓸 수 있습니다.';
  if (!url.hostname) return 'http:// 또는 https://로 시작하는 주소를 입력하세요(예: http://127.0.0.1:8317/v1).';
  if (url.username || url.password) return '주소에 사용자 이름이나 비밀번호를 넣을 수 없습니다. 키는 아래 키 칸에 넣으세요.';
  if (url.search || url.hash || text.includes('?') || text.includes('#')) return '주소에 ? 나 # 뒤 내용을 넣을 수 없습니다.';
  if (kind === 'gemini') {
    const path = url.pathname.toLowerCase();
    if (path.endsWith('/v1beta') || path.includes('/v1beta/')) {
      return 'Gemini 주소에는 /v1beta 경로를 넣지 않습니다. 호스트 주소만 입력하세요(예: https://generativelanguage.googleapis.com).';
    }
  }
  return undefined;
}

function concurrencyValue(text: string): number | null | 'bad' {
  const t = text.trim();
  if (!t) return null;
  if (!/^\d+$/.test(t)) return 'bad';
  const n = Number(t);
  return n >= 1 && n <= MAX_CONCURRENCY ? n : 'bad';
}

/** Problems that block saving, per field. `profiles` are the saved connections (for duplicate names). */
export function validate(form: ConnectionForm, profiles: ProfileView[]): FormErrors {
  const errors: FormErrors = {};
  const name = form.name.trim();
  if (!name) errors.name = '이름을 입력하세요.';
  else if (Array.from(name).length > MAX_NAME_CHARS) errors.name = `이름은 ${MAX_NAME_CHARS}자까지 쓸 수 있습니다.`;
  else if (/[\u0000-\u001f\u007f]/.test(name)) errors.name = '이름에 줄바꿈이나 제어 문자를 넣을 수 없습니다.';
  else if (/^p-[0-9a-f]{8}$/i.test(name)) errors.name = '"p-"와 16진수 8자리 모양의 이름은 쓸 수 없습니다(연결 id와 헷갈림).';
  else if (profiles.some((p) => p.id !== form.id && p.name.toLowerCase() === name.toLowerCase())) {
    errors.name = '같은 이름의 연결이 이미 있습니다.';
  }

  if (!urlLocked(form)) {
    const bad = addressError(form.baseUrl, form.kind);
    if (bad) errors.baseUrl = bad;
  }

  if (concurrencyValue(form.concurrency) === 'bad') errors.concurrency = `동시 실행 수는 1~${MAX_CONCURRENCY} 사이 정수입니다.`;

  const info = kindInfo(form.kind);
  if (form.id === null) {
    if (info.keyRequired && !form.key.trim()) errors.key = `${info.label} 연결은 키가 필요합니다.`;
  } else if (form.keyMode === 'replace' && !form.key.trim()) {
    errors.key = '새 키를 입력하세요.';
  }
  return errors;
}

export function hasErrors(errors: FormErrors): boolean {
  return Object.values(errors).some(Boolean);
}

/** What save_profile receives. key: null = keep, "" = delete, other = the new key (trimmed). */
export function toInput(form: ConnectionForm): ProfileInput {
  let key: string | null;
  if (form.id === null) key = form.key.trim() || null;
  else if (form.keyMode === 'keep') key = null;
  else if (form.keyMode === 'clear') key = '';
  else key = form.key.trim();
  const conc = concurrencyValue(form.concurrency);
  return {
    id: form.id,
    name: form.name.trim(),
    kind: form.kind,
    baseUrl: urlLocked(form) ? null : form.baseUrl.trim() || null,
    concurrency: typeof conc === 'number' ? conc : null,
    key,
    makeDefault: form.makeDefault,
  };
}

/** Unsaved changes compared with the saved connection (null = new form). */
export function isDirty(form: ConnectionForm, saved: ProfileView | null): boolean {
  const keyChange = form.keyMode === 'clear' || (form.keyMode === 'replace' && form.key.trim() !== '');
  if (!saved) {
    const typedUrl = form.baseUrl.trim() !== '' && !sameUrl(form.baseUrl, officialUrl(form.kind) ?? '');
    return form.name.trim() !== '' || form.key.trim() !== '' || form.concurrency.trim() !== '' || typedUrl || form.makeDefault;
  }
  return (
    form.name.trim() !== saved.name ||
    form.baseUrl.trim() !== saved.baseUrl ||
    form.concurrency.trim() !== (saved.concurrency === null ? '' : String(saved.concurrency)) ||
    (form.id !== null && keyChange) ||
    (form.makeDefault && !saved.isDefault)
  );
}

function ipv4(host: string): number[] | null {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(host);
  if (!m) return null;
  const parts = m.slice(1).map(Number);
  return parts.every((n) => n <= 255) ? parts : null;
}

/**
 * Plain http to a public address (spec §6.1): key and images would travel unencrypted. Mirrors
 * image_core::profiles::transport_warning (loopback, private, Tailscale 100.64/10, fd7a:115c:a1e0::/48, *.ts.net,
 * localhost, single-label names are trusted). Unparseable text gives false; validation reports it.
 */
export function transportWarning(raw: string): boolean {
  let url: URL;
  try {
    url = new URL(raw.trim());
  } catch {
    return false;
  }
  if (url.protocol !== 'http:') return false;
  const host = url.hostname.replace(/^\[|\]$/g, '').toLowerCase();
  if (!host) return false;
  const v4 = ipv4(host);
  if (v4) {
    const [a, b] = v4;
    const trusted = a === 127 || a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 100 && b >= 64 && b <= 127);
    return !trusted;
  }
  if (host.includes(':')) return !(host === '::1' || host.startsWith('fd7a:115c:a1e0:'));
  const trusted = !host.includes('.') || host.endsWith('.localhost') || host.endsWith('.ts.net');
  return !trusted;
}
