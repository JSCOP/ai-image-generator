// Development-only fake backend: `pnpm dev` + `?mock=1` (or `?mock=empty` for a first run without any connection, `?mock=broken` for an unreadable profiles.json,
// `?mock=env` for a CLIPROXY_BASE_URL user environment connection that wins over the default profile).
// main.ts imports this file only under import.meta.env.DEV, so production builds never contain it or its images.
// No real file, key or provider is touched; everything lives in memory.

import { clearMocks, mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type { Channel } from '@tauri-apps/api/core';
import type {
  Category,
  ConnectionReport,
  ConnectionsView,
  GalleryItem,
  GalleryView,
  GenerateInput,
  ModelView,
  ProfileInput,
  ProfileView,
  RunEvent,
  RunSummary,
  ServiceError,
  Settings,
} from '../lib/types';
import img1 from './mock/1.png';
import img2 from './mock/2.png';
import img3 from './mock/3.png';
import img4 from './mock/4.png';

const OPAQUE = [img1, img2, img3];
const TRANSPARENT = img4;

const HOME = 'C:\\Users\\demo\\AppData\\Roaming\\ImageStudio';
const WORK = 'C:\\Users\\demo\\Pictures\\Image Studio';
const PROJECT = 'D:\\projects\\brand-kit';
const MISSING = 'E:\\usb-drive\\renders';

const KIND_LABELS = {
  cliproxy: 'CLIProxyAPI',
  openai: 'OpenAI 공식',
  gemini: 'Gemini 공식',
  xai: 'xAI 공식',
  openai_compatible: 'OpenAI 호환(기타)',
} as const;

const OFFICIAL_URLS: Partial<Record<keyof typeof KIND_LABELS, string>> = {
  openai: 'https://api.openai.com/v1',
  gemini: 'https://generativelanguage.googleapis.com',
  xai: 'https://api.x.ai/v1',
};

const MODELS: ModelView[] = [
  { id: 'gpt-image-2.5', provider: 'OpenAI', references: true, maxReferences: 4, transparency: false },
  { id: 'gpt-image-2.5-flare', provider: 'OpenAI', references: true, maxReferences: 4, transparency: true },
  { id: 'gpt-image-2.5-sunburst', provider: 'OpenAI', references: true, maxReferences: 4, transparency: true },
  { id: 'gpt-image-2', provider: 'OpenAI', references: true, maxReferences: 4, transparency: false },
  { id: 'gemini-3.1-flash-image', provider: 'Google Gemini', references: true, maxReferences: 4, transparency: false },
  { id: 'grok-imagine-image-2.0', provider: 'xAI Grok', references: false, maxReferences: 0, transparency: false },
];

const now = () => Date.now() / 1000;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const fail = (category: Category, message: string): ServiceError => ({ category, message });

function slug(prompt: string): string {
  const words = prompt
    .split(/\s+/)
    .map((w) => w.replace(/[^\p{L}\p{N}_-]/gu, ''))
    .filter(Boolean)
    .slice(0, 5);
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, '0');
  const stamp = `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
  return `${(words.join('_') || 'image').slice(0, 64)}-${stamp}`;
}

/** Same path always gets the same picture; transparent results get the transparent one. */
const pictureOf = new Map<string, string>();
function pictureFor(path: string): string {
  const known = pictureOf.get(path);
  if (known) return known;
  let h = 0;
  for (const ch of path) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return OPAQUE[h % OPAQUE.length];
}

function galleryItems(): GalleryItem[] {
  const base = now();
  const rows: [string, string, GalleryItem['status'], string, string, number, string | null][] = [
    // topic, prompt, status, model, size, age in hours, error
    ['아늑한_독서_공간-20261018-140509', '아늑한 독서 공간, 오후 햇살이 드는 창가와 담요', 'complete', 'gpt-image-2.5', '1920x1080', 0.4, null],
    ['아늑한_독서_공간-20261018-140509', '아늑한 독서 공간, 오후 햇살이 드는 창가와 담요', 'complete', 'gpt-image-2.5', '1920x1080', 0.4, null],
    ['아늑한_독서_공간-20261018-140509', '아늑한 독서 공간, 오후 햇살이 드는 창가와 담요', 'failed', 'gpt-image-2.5', '1920x1080', 0.4, '사용 한도 초과: billing_hard_limit_reached'],
    ['product_mug-20261018-111200', 'product shot, ceramic mug on a wooden table 🍵 soft studio light', 'complete', 'gpt-image-2', '1024x1024', 3, null],
    ['product_mug-20261018-111200', 'product shot, ceramic mug on a wooden table 🍵 soft studio light', 'missing', 'gpt-image-2', '1024x1024', 3, null],
    ['logo_mark-20261017-180210', '투명 배경의 앱 아이콘, 보라색 사각 프레임과 반짝임', 'complete', 'gpt-image-2.5-flare', '1024x1024', 20, null],
    ['airport_dashboard-20261017-091000', '공항 운영 대시보드 어두운 화면, 지도와 경보 패널\n두 번째 줄 설명', 'complete', 'gemini-3.1-flash-image', '1920x960', 29, null],
    ['airport_dashboard-20261017-091000', '공항 운영 대시보드 어두운 화면, 지도와 경보 패널\n두 번째 줄 설명', 'pending', 'gemini-3.1-flash-image', '1920x960', 29, null],
    ['legacy-topic', 'old python run without status', 'legacy', 'gpt-image-1.5', '1536x1024', 24 * 5, null],
    ['legacy-topic', 'old python run without status', 'legacy', 'gpt-image-1.5', '1536x1024', 24 * 5, null],
    ['brand_poster-20261003-101500', '브랜드 포스터 시안 — 아주 긴 한국어 프롬프트가 카드에서 말줄임으로 잘리는지 확인하기 위한 문장입니다 😀👨‍👩‍👧', 'complete', 'grok-imagine-image-2.0', '1080x1920', 24 * 15, null],
    ['brand_poster-20261003-101500', '브랜드 포스터 시안 — 아주 긴 한국어 프롬프트가 카드에서 말줄임으로 잘리는지 확인하기 위한 문장입니다 😀👨‍👩‍👧', 'complete', 'grok-imagine-image-2.0', '1080x1920', 24 * 15, null],
  ];
  const counter = new Map<string, number>();
  return rows.map(([topic, prompt, status, model, size, hours, error], i) => {
    const n = (counter.get(topic) ?? 0) + 1;
    counter.set(topic, n);
    const root = i >= 9 ? PROJECT : WORK;
    const imagePath = `${root}\\ImageGallery\\output\\${topic}\\${topic}_${n}.png`;
    const transparent = model.endsWith('flare');
    if (transparent) pictureOf.set(imagePath, TRANSPARENT);
    return {
      imagePath,
      recordPath: `${root}\\ImageGallery\\metadata\\${topic}\\${topic}_${n}.json`,
      folder: `${root}\\ImageGallery`,
      topic,
      status,
      prompt,
      imageModel: model,
      responseModel: status === 'complete' ? `${model}-2026-10-01` : null,
      size,
      background: transparent ? 'transparent' : 'auto',
      transparentRatio: transparent ? 0.62 : null,
      error,
      modified: base - hours * 3600 - i,
      quality: 'medium',
      references: [],
      providerSize: status === 'complete' ? (size === '1024x1024' ? '1448x1086' : size) : null,
      savedSize: status === 'complete' ? size : null,
      requestId: status === 'complete' ? `req_mock${i + 1}` : null,
    };
  });
}

interface MockRun {
  cancelled: boolean;
}

/** Installs the fake backend. `mode` is the ?mock= value ("empty" = no connection yet, "broken" = connections fail). */
export function installMock(mode: string | null): void {
  const empty = mode === 'empty';
  let profiles: ProfileView[] = empty
    ? []
    : [
        {
          id: 'p-f55fc2cc',
          name: 'local',
          kind: 'cliproxy',
          kindLabel: KIND_LABELS.cliproxy,
          baseUrl: 'http://127.0.0.1:8317/v1',
          concurrency: null,
          isDefault: true,
          keyConfigured: true,
          warning: null,
          lastGeneration: { ok: true, at: now() - 1500, model: 'gpt-image-2.5', category: null, provider_code: null },
        },
        {
          id: 'p-0a1b2c3d',
          name: 'OpenAI 회사 키',
          kind: 'openai',
          kindLabel: KIND_LABELS.openai,
          baseUrl: 'https://api.openai.com/v1',
          concurrency: 8,
          isDefault: false,
          keyConfigured: true,
          warning: null,
          lastGeneration: { ok: false, at: now() - 86400, model: 'gpt-image-2', category: 'quota', provider_code: 'billing_hard_limit_reached' },
        },
      ];
  let settings: Settings = { workDir: WORK, galleryFolders: empty ? [] : [PROJECT, MISSING], lastProfile: null, lastModel: 'gpt-image-2.5' };
  const items = empty ? [] : galleryItems();
  const runs = new Map<string, MockRun>();
  let nextId = 1;
  const envConn = mode === 'env' ? { baseUrl: 'http://100.98.54.122:8317/v1', source: 'user_env', keyConfigured: true } : null;

  const view = (): ConnectionsView => {
    const def = profiles.find((p) => p.isDefault);
    if (envConn) return { profiles, env: envConn, active: 'env' };
    return { profiles, env: null, active: def ? def.name : null };
  };

  const find = (name: string | null): ProfileView => {
    const p = name ? profiles.find((x) => x.id === name || x.name.toLowerCase() === name.toLowerCase()) : profiles.find((x) => x.isDefault);
    if (!p) throw fail('invalid_request', name ?`프로필을 찾을 수 없습니다: ${name}` : '연결이 없습니다. 연결을 먼저 추가하세요.');
    return p;
  };

  function saveProfile(input: ProfileInput): ProfileView {
    const name = input.name.trim();
    if (!name) throw fail('invalid_request', '이름을 입력하세요');
    const clash = profiles.find((p) => p.name.toLowerCase() === name.toLowerCase() && p.id !== input.id);
    if (clash) throw fail('invalid_request', `같은 이름의 연결이 있습니다: ${name}`);
    const baseUrl = input.baseUrl?.trim() || OFFICIAL_URLS[input.kind];
    if (!baseUrl) throw fail('invalid_request', '주소를 입력하세요');
    const old = input.id ? profiles.find((p) => p.id === input.id) : undefined;
    if (input.id && !old) throw fail('not_found', `프로필을 찾을 수 없습니다: ${input.id}`);
    if (old && old.kind !== input.kind) throw fail('invalid_request', '연결 종류는 바꿀 수 없습니다');
    const keyConfigured = input.key === null ? (old?.keyConfigured ?? false) : input.key.trim() !== '';
    const saved: ProfileView = {
      id: old?.id ?? `p-${(0x10000000 + nextId++).toString(16)}`,
      name,
      kind: input.kind,
      kindLabel: KIND_LABELS[input.kind],
      baseUrl,
      concurrency: input.concurrency,
      isDefault: old?.isDefault ?? false,
      keyConfigured,
      warning: baseUrl.startsWith('http://') && !/\/\/(127\.|localhost|10\.|192\.168\.|100\.)/.test(baseUrl) ? '암호화되지 않은 http 주소입니다. 키가 그대로 전송됩니다.' : null,
      lastGeneration: old?.lastGeneration ?? null,
    };
    profiles = old ? profiles.map((p) => (p.id === old.id ? saved : p)) : [...profiles, saved];
    if (input.makeDefault || profiles.length === 1) profiles = profiles.map((p) => ({ ...p, isDefault: p.id === saved.id }));
    return profiles.find((p) => p.id === saved.id)!;
  }

  function check(name: string | null): ConnectionReport {
    if (!name && envConn) {
      return {
        profile: null,
        kind: 'cliproxy',
        base_url: envConn.baseUrl,
        key_configured: envConn.keyConfigured,
        reachable: false,
        authenticated: false,
        models_listed: false,
        image_models: [],
        last_generation: null,
        warning: null,
        error: { category: 'timeout', provider_code: null, http_status: null, retryable: true, message: 'GET http://100.98.54.122:8317/v1/models: 15초 안에 응답이 없습니다' },
      };
    }
    const p = find(name);
    const broken = p.kind === 'openai';
    return {
      profile: p.name,
      kind: p.kind,
      base_url: p.baseUrl,
      key_configured: p.keyConfigured,
      reachable: true,
      authenticated: !broken,
      models_listed: !broken,
      image_models: broken ? [] : MODELS.map((m) => m.id),
      last_generation: p.lastGeneration,
      warning: p.warning,
      error: broken ? { category: 'auth', provider_code: 'invalid_api_key', http_status: 401, retryable: false, message: 'HTTP 401: Incorrect API key provided' } : null,
    };
  }

  async function generate(input: GenerateInput, channel: Channel<RunEvent>): Promise<RunSummary> {
    const send = (ev: RunEvent) => channel.onmessage(ev);
    find(input.profile);
    if (!input.prompt.trim()) throw fail('invalid_request', '프롬프트를 입력하세요');
    if (input.count < 1 || input.count > 50) throw fail('invalid_request', '장수는 1~50장입니다');
    if (input.references.length > 4) throw fail('invalid_request', '참조 이미지는 최대 4장입니다');
    const model = MODELS.find((m) => m.id === input.imageModel);
    if (input.background === 'transparent' && !model?.transparency) {
      throw fail('unsupported', `${input.imageModel} 모델은 투명 배경을 지원하지 않습니다`);
    }
    const topic = input.topic ?? slug(input.prompt);
    const outputDir = `${WORK}\\ImageGallery\\output\\${topic}`;
    const runId = `run-mock${nextId++}`;
    const run: MockRun = { cancelled: false };
    runs.set(runId, run);
    const started = performance.now();
    send({ event: 'started', data: { runId, total: input.count, outputDir, topic } });

    const outputs: string[] = [];
    const failures: RunSummary['failures'] = [];
    const concurrency = 4;
    let next = 1;
    const one = async (index: number) => {
      // Engine naming: one image = "<topic>.png", more = "<topic>_001.png".
      const output = input.count === 1 ? `${outputDir}\\${topic}.png` : `${outputDir}\\${topic}_${String(index).padStart(3, '0')}.png`;
      if (input.background === 'transparent') pictureOf.set(output, TRANSPARENT);
      if (run.cancelled) {
        failures.push({ index, reason: '취소되었습니다', category: 'cancelled', providerCode: null });
        return;
      }
      if (input.resume && index === 1) {
        send({ event: 'imageSkipped', data: { index, output } });
        outputs.push(output);
        return;
      }
      send({ event: 'imageStarted', data: { index, output } });
      const t0 = performance.now();
      await sleep(900 + ((index * 337) % 1400));
      if (index === 2 && !run.cancelled) {
        send({ event: 'imageRetry', data: { index, attempt: 1, delaySec: 2, reason: 'HTTP 429: Too many requests, please slow down' } });
        await sleep(2000);
      }
      if (run.cancelled) {
        send({ event: 'imageFailed', data: { index, reason: '취소되었습니다' } });
        failures.push({ index, reason: '취소되었습니다', category: 'cancelled', providerCode: null });
        return;
      }
      if (input.count >= 4 && index === input.count) {
        const reason = 'HTTP 429: You exceeded your current quota (billing_hard_limit_reached)';
        send({ event: 'imageFailed', data: { index, reason } });
        failures.push({ index, reason, category: 'quota', providerCode: 'billing_hard_limit_reached' });
        return;
      }
      send({ event: 'imageFinished', data: { index, output, elapsedSec: Math.round((performance.now() - t0) / 10) / 100 } });
      outputs.push(output);
    };
    const worker = async () => {
      while (next <= input.count) await one(next++);
    };
    await Promise.all(Array.from({ length: Math.min(concurrency, input.count) }, worker));
    runs.delete(runId);
    failures.sort((a, b) => a.index - b.index);
    const summary: RunSummary = {
      ok: failures.length === 0,
      outputs,
      failures,
      topic,
      outputDir,
      elapsedSec: Math.round((performance.now() - started) / 10) / 100,
      cancelled: run.cancelled,
      warnings: [],
    };
    // Records appear in the gallery like the engine's (finished images and failed attempts; cancelled-before-start ones have none).
    const prompt = input.avoid.trim() ? `${input.prompt.trim()}\nAvoid: ${input.avoid.trim()}` : input.prompt.trim();
    const record = (output: string, status: GalleryItem['status'], error: string | null): GalleryItem => ({
      imagePath: output,
      recordPath: output.replace('\\output\\', '\\metadata\\') + '.json',
      folder: `${WORK}\\ImageGallery`,
      topic,
      status,
      prompt,
      imageModel: input.imageModel,
      responseModel: status === 'complete' ? `${input.imageModel}-2026-10-01` : null,
      size: input.size,
      background: input.background,
      transparentRatio: status === 'complete' && input.background === 'transparent' ? 0.448 : null,
      error,
      modified: now(),
      quality: input.quality,
      references: [...input.references],
      providerSize: status === 'complete' ? '1536x1024' : null,
      savedSize: status === 'complete' ? input.size : null,
      requestId: status === 'complete' ? `req_mock${nextId++}` : null,
    });
    const known = (path: string) => items.findIndex((i) => i.imagePath === path);
    const put = (item: GalleryItem) => {
      const at = known(item.imagePath);
      if (at >= 0) items.splice(at, 1);
      items.unshift(item);
    };
    for (const output of outputs) put(record(output, 'complete', null));
    for (const f of failures) {
      if (f.category === 'cancelled') continue;
      const output = input.count === 1 ? `${outputDir}\\${topic}.png` : `${outputDir}\\${topic}_${String(f.index).padStart(3, '0')}.png`;
      put(record(output, 'failed', f.reason));
    }
    send({ event: 'finished', data: { summary } });
    return summary;
  }

  function galleryView(): GalleryView {
    const roots = [WORK, ...settings.galleryFolders];
    return {
      items,
      folders: roots.map((path, i) => ({ path, galleryRoot: `${path}\\ImageGallery`, available: path !== MISSING, isWorkDir: i === 0 })),
      truncated: false,
    };
  }

  const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
    app_info: () => ({ version: '0.1.0-mock', home: HOME, workDir: WORK }),
    get_settings: () => settings,
    save_settings: (a) => {
      settings = a.settings as Settings;
      return null;
    },
    connections: () => {
      if (mode === 'broken') throw fail('invalid_request', `${HOME}\\profiles.json을 읽을 수 없습니다: line 4 column 3, 같은 이름의 연결이 있습니다: local`);
      return view();
    },
    save_profile: (a) => saveProfile(a.input as ProfileInput),
    remove_profile: (a) => {
      const p = find(a.id as string);
      profiles = profiles.filter((x) => x.id !== p.id);
      return null;
    },
    set_default_profile: (a) => {
      const id = (a.id as string | null) ?? null;
      if (id) find(id);
      profiles = profiles.map((p) => ({ ...p, isDefault: p.id === id }));
      return null;
    },
    check_profile: (a) => check((a.name as string | null) ?? null),
    list_models: (a) => (check((a.name as string | null) ?? null).models_listed ? MODELS : []),
    start_generation: (a) => generate(a.input as GenerateInput, a.onEvent as Channel<RunEvent>),
    cancel_generation: (a) => {
      const run = runs.get(a.runId as string);
      if (run) run.cancelled = true;
      return Boolean(run);
    },
    gallery: () => galleryView(),
    add_gallery_folder: (a) => {
      const path = a.path as string;
      if (!settings.galleryFolders.some((f) => f.toLowerCase() === path.toLowerCase()) && path.toLowerCase() !== WORK.toLowerCase()) {
        settings = { ...settings, galleryFolders: [...settings.galleryFolders, path] };
      }
      return settings;
    },
    remove_gallery_folder: (a) => {
      const path = a.path as string;
      if (path.toLowerCase() === WORK.toLowerCase()) throw fail('invalid_request', '작업 폴더는 제거할 수 없습니다. 설정에서 바꾸세요.');
      settings = { ...settings, galleryFolders: settings.galleryFolders.filter((f) => f.toLowerCase() !== path.toLowerCase()) };
      return settings;
    },
    // Plugins: folder/file picker returns a fixed path; opener does nothing.
    inspect_reference: (a) => {
      const path = a.path as string;
      const name = path.split(/[\\/]/).pop() ?? path;
      if (!/\.(png|jpe?g|webp|gif)$/i.test(name)) throw fail('invalid_request', `이미지 파일이 아닙니다(PNG, JPEG, WebP, GIF): ${path}`);
      if (/huge/i.test(name)) throw fail('invalid_request', `참조 이미지는 장당 10MB 이하여야 합니다: ${path}`);
      return { path, name, bytes: 245_000 + name.length * 1000, width: 1024, height: 1024 };
    },
    'plugin:dialog|open': (a) => {
      const options = (a.options ?? {}) as { directory?: boolean; multiple?: boolean };
      if (options.directory) return 'D:\\projects\\new-folder';
      const files = [`${PROJECT}\\ImageGallery\\output\\legacy-topic\\legacy-topic_1.png`, `D:\\refs\\mug-reference.jpg`];
      return options.multiple ? files : files[0];
    },
    'plugin:opener|reveal_item_in_dir': () => null,
    'plugin:opener|open_path': () => null,
  };

  clearMocks();
  mockWindows('main');
  mockIPC(
    async (cmd, args) => {
      const handler = handlers[cmd];
      if (!handler) throw fail('internal', `목업에 없는 명령입니다: ${cmd}`);
      await sleep(cmd === 'check_profile' ? 700 : 120);
      return handler((args ?? {}) as Record<string, unknown>);
    },
    { shouldMockEvents: true },
  );
  // Local paths -> bundled mock pictures (the real app uses the asset protocol).
  const internals = (window as unknown as { __TAURI_INTERNALS__: { convertFileSrc: (path: string) => string } }).__TAURI_INTERNALS__;
  internals.convertFileSrc = (path: string) => pictureFor(path);
  console.info(`[mock] Image Studio 목업 백엔드 사용 중 (mode=${mode ?? '1'})`);
}
