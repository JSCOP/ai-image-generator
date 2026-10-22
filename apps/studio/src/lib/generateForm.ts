// Generate form (pure): sizes, model capabilities, reference limits, the GenerateInput sent to start_generation,
// reusing a gallery record, the history list and the result cards. The Rust service checks everything again.

import { cardText, type Card, type CardStatus } from './run';
import { categoryLabel, formatElapsed } from './format';
import { ENV_NAME } from './connection';
import type { ConnectionsView, Fit, FolderView, GalleryItem, GenerateInput, ModelView, ReferenceView, ServiceError } from './types';

/** Images per run (service MAX_COUNT). */
export const MAX_COUNT = 50;
/** References per run (service MAX_REFERENCES). */
export const MAX_REFERENCES = 4;
/** Largest reference file (service MAX_REFERENCE_BYTES). */
export const MAX_REFERENCE_BYTES = 10 * 1024 * 1024;
/** Sides of a custom size. */
export const MIN_SIDE = 16;
export const MAX_SIDE = 8192;
/** `connection` value of the environment connection (same word as ConnectionsView.active). */
export const ENV_CHOICE = 'env';
/** `size` value when the custom field is used. */
export const CUSTOM_SIZE = 'custom';

export type Quality = 'low' | 'medium' | 'high';

export const QUALITIES: { value: Quality; label: string }[] = [
  { value: 'low', label: '낮음' },
  { value: 'medium', label: '보통' },
  { value: 'high', label: '높음' },
];

export interface SizePreset {
  value: string;
  label: string;
  ratio: string;
}

export const SIZE_PRESETS: SizePreset[] = [
  { value: '1920x1080', label: '1920×1080', ratio: '16:9' },
  { value: '1024x1024', label: '1024×1024', ratio: '1:1' },
  { value: '1080x1920', label: '1080×1920', ratio: '9:16' },
  { value: '1536x1024', label: '1536×1024', ratio: '3:2' },
  { value: '1024x1536', label: '1024×1536', ratio: '2:3' },
  { value: '2048x2048', label: '2048×2048', ratio: '1:1' },
  { value: '3840x2160', label: '3840×2160', ratio: '4K' },
];

export interface GenerateForm {
  /** Profile name, ENV_CHOICE, or null (nothing to choose from). */
  connection: string | null;
  prompt: string;
  /** The text the user wrote before "영어로 다듬기" replaced `prompt`; null when the prompt is their own. */
  originalPrompt: string | null;
  avoid: string;
  model: string;
  /** A preset value or CUSTOM_SIZE. */
  size: string;
  customSize: string;
  quality: Quality;
  count: number;
  references: ReferenceView[];
  /** Model-made transparent background (background "transparent"), else "auto". */
  transparent: boolean;
  /** How a result of another shape is saved; never cropping by default. */
  fit: Fit;
  /** Set by "이어서 생성": the topic to continue. */
  topic: string | null;
  resume: boolean;
}

export function emptyForm(): GenerateForm {
  return {
    connection: null,
    prompt: '',
    originalPrompt: null,
    avoid: '',
    model: '',
    size: '1920x1080',
    customSize: '',
    quality: 'high',
    count: 1,
    references: [],
    transparent: false,
    fit: 'contain',
    topic: null,
    resume: false,
  };
}

// ---- sizes ----

export type SizeResult = { ok: true; size: string } | { ok: false; message: string };

/** "1600 × 900" -> "1600x900"; each side 16..=8192. */
export function parseSize(text: string): SizeResult {
  const m = /^\s*(\d+)\s*[xX×]\s*(\d+)\s*$/.exec(text);
  if (!m) return { ok: false, message: '"가로x세로" 형식으로 입력하세요(예: 1600x900).' };
  const w = Number(m[1]);
  const h = Number(m[2]);
  if (w < MIN_SIDE || h < MIN_SIDE || w > MAX_SIDE || h > MAX_SIDE) {
    return { ok: false, message: `가로와 세로는 ${MIN_SIDE}~${MAX_SIDE} 사이여야 합니다.` };
  }
  return { ok: true, size: `${w}x${h}` };
}

/** The size the run would use, or null when the custom field is not valid. */
export function sizeOf(form: GenerateForm): string | null {
  if (form.size !== CUSTOM_SIZE) return form.size;
  const parsed = parseSize(form.customSize);
  return parsed.ok ? parsed.size : null;
}

/** Width / height of "WxH" (1 when unknown), for card shapes. */
export function aspectOf(size: string | null | undefined): number {
  const m = /^(\d+)x(\d+)$/.exec(size ?? '');
  if (!m) return 1;
  const ratio = Number(m[1]) / Number(m[2]);
  return Number.isFinite(ratio) && ratio > 0 ? ratio : 1;
}

// ---- model capabilities ----

export interface Capabilities {
  references: boolean;
  maxReferences: number;
  /** Why references are off, else "". */
  referencesReason: string;
  transparency: boolean;
  transparencyReason: string;
}

const NO_MODEL = '모델을 먼저 고르세요.';
const NO_TRANSPARENCY = '이 모델은 투명 배경을 지원하지 않습니다.';
const NO_REFERENCES = '이 모델은 참조 이미지를 쓸 수 없습니다.';

export function capabilities(model: ModelView | undefined): Capabilities {
  if (!model) {
    return { references: false, maxReferences: 0, referencesReason: NO_MODEL, transparency: false, transparencyReason: NO_MODEL };
  }
  const max = model.references ? Math.min(model.maxReferences, MAX_REFERENCES) : 0;
  return {
    references: max > 0,
    maxReferences: max,
    referencesReason: max > 0 ? '' : NO_REFERENCES,
    transparency: model.transparency,
    transparencyReason: model.transparency ? '' : NO_TRANSPARENCY,
  };
}

// ---- references ----

/** Same Windows path: case and slash direction ignored. */
export function samePath(a: string, b: string): boolean {
  const norm = (p: string) => p.replace(/\//g, '\\').replace(/\\+$/, '').toLowerCase();
  return norm(a) === norm(b);
}

/** Adds checked references up to `max`; `rejected` says why each other file was left out. */
export function addReferences(
  current: ReferenceView[],
  incoming: ReferenceView[],
  max: number,
): { references: ReferenceView[]; rejected: string[] } {
  const references = [...current];
  const rejected: string[] = [];
  for (const ref of incoming) {
    if (max <= 0) rejected.push(`${NO_REFERENCES.slice(0, -1)}: ${ref.name}`);
    else if (references.some((r) => samePath(r.path, ref.path))) rejected.push(`이미 추가한 이미지입니다: ${ref.name}`);
    else if (ref.bytes > MAX_REFERENCE_BYTES) rejected.push(`참조 이미지는 장당 10MB 이하여야 합니다: ${ref.name}`);
    else if (references.length >= max) rejected.push(`참조 이미지는 최대 ${max}장입니다: ${ref.name}`);
    else references.push(ref);
  }
  return { references, rejected };
}

// ---- connections and models ----

export type ConnectionState = 'loading' | 'error' | 'none' | 'ready';

/** No connection at all = the screen shows "연결을 먼저 추가하세요" instead of the generate button (Review Focus 5). */
export function connectionState(view: ConnectionsView | null, error: ServiceError | null): ConnectionState {
  if (view) return view.profiles.length === 0 && !view.env ? 'none' : 'ready';
  return error ? 'error' : 'loading';
}

/** The connection a run uses unless the user picks another: the active one, else the first profile. */
export function defaultConnection(view: ConnectionsView | null): string | null {
  if (!view) return null;
  if (view.active) return view.active;
  if (view.profiles.length > 0) return view.profiles[0].name;
  return view.env ? ENV_CHOICE : null;
}

export interface ConnectionChoice {
  value: string;
  label: string;
}

export function connectionChoices(view: ConnectionsView | null): ConnectionChoice[] {
  if (!view) return [];
  const out: ConnectionChoice[] = [];
  if (view.env) out.push({ value: ENV_CHOICE, label: ENV_NAME });
  for (const p of view.profiles) out.push({ value: p.name, label: `${p.name} · ${p.kindLabel}${p.isDefault ? ' (기본)' : ''}` });
  return out;
}

/** The current model if listed, else the last used one if listed, else the first; "" for an empty list. */
export function defaultModel(models: ModelView[], current: string, last: string | null): string {
  if (models.some((m) => m.id === current)) return current;
  if (last && models.some((m) => m.id === last)) return last;
  return models[0]?.id ?? '';
}

/** Chat model suggested for "영어로 다듬기". */
export const DEFAULT_TEXT_MODEL = 'gpt-6-luna';

/**
 * Chat model for enhancement: the current one if listed, else the saved one if listed, else gpt-6-luna if listed,
 * else the first. Without a list (not loaded or failed) the current one or gpt-6-luna.
 */
export function pickTextModel(models: string[], current: string, saved: string | null): string {
  if (models.length === 0) return current || saved || DEFAULT_TEXT_MODEL;
  for (const id of [current, saved, DEFAULT_TEXT_MODEL]) {
    if (id && models.includes(id)) return id;
  }
  return models[0];
}

const TEXT_MAKERS: [pattern: RegExp, label: string][] = [
  [/^(gpt-|chatgpt-|o\d)/i, 'OpenAI'],
  [/^claude-/i, 'Claude'],
  [/^gemini-/i, 'Gemini'],
  [/^grok-/i, 'xAI Grok'],
  [/^kimi-/i, 'Kimi'],
];

/** Group order and label of a chat model; unknown makers come last without a label. */
function textMaker(id: string): [rank: number, label: string] {
  const at = TEXT_MAKERS.findIndex(([pattern]) => pattern.test(id));
  return at < 0 ? [TEXT_MAKERS.length, ''] : [at, TEXT_MAKERS[at][1]];
}

/**
 * Every chat model of the connection for the "다듬기 모델" select, grouped by maker (OpenAI, Claude, Gemini, Grok, Kimi,
 * then the rest by name). A chosen model missing from the list stays as the last option; no list offers gpt-6-luna.
 */
export function textModelOptions(models: string[], current: string): { value: string; label: string }[] {
  const ids = [...new Set(models)];
  if (current && !ids.includes(current)) ids.push(current);
  if (ids.length === 0) ids.push(DEFAULT_TEXT_MODEL);
  const listed = new Set(models);
  const keyed = ids.map((id) => ({ id, maker: textMaker(id), extra: !listed.has(id) && models.length > 0 }));
  keyed.sort((a, b) => Number(a.extra) - Number(b.extra) || a.maker[0] - b.maker[0] || a.id.localeCompare(b.id));
  return keyed.map(({ id, maker }) => ({ value: id, label: maker[1] ? `${id} · ${maker[1]}` : id }));
}

// ---- form -> GenerateInput ----

export type InputResult = { ok: true; input: GenerateInput } | { ok: false; errors: string[] };

/** Every reason the run cannot start, or the input for start_generation. */
export function toInput(form: GenerateForm, models: ModelView[]): InputResult {
  const errors: string[] = [];
  if (!form.connection) errors.push('연결을 고르세요.');
  if (!form.prompt.trim()) errors.push('프롬프트를 입력하세요.');
  const model = models.find((m) => m.id === form.model);
  if (!model) errors.push('모델을 고르세요.');
  const size = sizeOf(form);
  if (!size) errors.push(form.size === CUSTOM_SIZE ? `크기: ${(parseSize(form.customSize) as { message: string }).message}` : '크기를 고르세요.');
  if (!Number.isInteger(form.count) || form.count < 1 || form.count > MAX_COUNT) errors.push(`장수는 1장부터 ${MAX_COUNT}장까지입니다.`);
  if (model) {
    const caps = capabilities(model);
    if (form.transparent && !caps.transparency) errors.push(NO_TRANSPARENCY);
    if (form.references.length > 0 && !caps.references) errors.push(NO_REFERENCES);
    else if (form.references.length > caps.maxReferences) errors.push(`이 모델은 참조 이미지를 ${caps.maxReferences}장까지 받습니다.`);
  }
  if (form.references.some((r) => r.bytes > MAX_REFERENCE_BYTES)) errors.push('참조 이미지는 장당 10MB 이하여야 합니다.');
  if (errors.length > 0) return { ok: false, errors };
  const resume = form.resume && !!form.topic;
  return {
    ok: true,
    input: {
      profile: form.connection === ENV_CHOICE ? null : form.connection,
      prompt: form.prompt.trim(),
      avoid: form.avoid.trim(),
      originalPrompt: form.originalPrompt?.trim() || null,
      imageModel: form.model,
      size: size as string,
      quality: form.quality,
      count: form.count,
      references: form.references.map((r) => r.path),
      background: form.transparent ? 'transparent' : 'auto',
      fit: form.fit,
      topic: resume ? form.topic : null,
      resume,
    },
  };
}

// ---- fit ----

export const FITS: { value: Fit; label: string; hint: string }[] = [
  { value: 'contain', label: '여백 채움', hint: '그림을 자르지 않고 정한 크기에 맞춥니다. 모양이 다르면 남는 곳은 투명한 여백이 됩니다.' },
  { value: 'original', label: '원본 크기', hint: '모델이 보낸 그림을 크기 그대로 저장합니다. 정한 크기와 다를 수 있습니다.' },
  { value: 'cover', label: '잘라 채움', hint: '정한 크기를 꽉 채우도록 가운데만 남기고 넘치는 부분을 잘라냅니다.' },
];

export function isFit(value: string | null | undefined): value is Fit {
  return FITS.some((f) => f.value === value);
}

// ---- reusing a record ----

/** Undoes the service's compose(): "prompt\nAvoid: avoid" (the last Avoid line). */
export function splitPrompt(text: string): { prompt: string; avoid: string } {
  const at = text.lastIndexOf('\nAvoid: ');
  if (at < 0) return { prompt: text, avoid: '' };
  return { prompt: text.slice(0, at), avoid: text.slice(at + '\nAvoid: '.length) };
}

/** Form fields a record gives back; references come as paths and are checked again before use. */
export interface ItemSettings {
  prompt: string;
  originalPrompt: string | null;
  avoid: string;
  model: string;
  size: string;
  customSize: string;
  quality: Quality;
  transparent: boolean;
  fit: Fit;
  referencePaths: string[];
  topic: string | null;
  resume: boolean;
}

/** "이 설정 다시 쓰기". */
export function formFromItem(item: GalleryItem): ItemSettings {
  const { prompt, avoid } = splitPrompt(item.prompt);
  const preset = SIZE_PRESETS.some((p) => p.value === item.size);
  const quality = QUALITIES.find((q) => q.value === item.quality)?.value ?? 'high';
  return {
    prompt,
    originalPrompt: item.originalPrompt ?? null,
    avoid,
    model: item.imageModel,
    size: preset ? item.size : CUSTOM_SIZE,
    customSize: preset ? '' : item.size,
    quality,
    transparent: item.background === 'transparent',
    fit: isFit(item.fit) ? item.fit : 'contain',
    referencePaths: [...item.references],
    topic: null,
    resume: false,
  };
}

/** Number at the end of an engine output name ("topic_003.png" -> 3); a single image ("topic.png") has none. */
export function imageNumber(path: string): number | null {
  const m = /_(\d+)\.[^.\\/]+$/.exec(path);
  return m ? Number(m[1]) : null;
}

/**
 * "이어서 생성": the same settings, topic and resume, and a count that covers every image number of the group
 * (numbered names mean the run had at least 2 images; one image is named without a number).
 */
export function resumeFrom(group: GalleryItem[]): ItemSettings & { count: number } {
  const first = group[0];
  const numbers = group.map((i) => imageNumber(i.imagePath)).filter((n): n is number => n !== null);
  let count = Math.max(group.length, ...numbers, 1);
  if (numbers.length > 0) count = Math.max(count, 2);
  // Resuming needs the fit the images were made with. Older records hold what was applied ("none") or nothing;
  // the app cropped then, or padded transparent images.
  const fit: Fit = isFit(first.fit) ? first.fit : first.background === 'transparent' ? 'contain' : 'cover';
  return { ...formFromItem(first), fit, topic: first.topic, resume: true, count: Math.min(count, MAX_COUNT) };
}

/** Why "이어서 생성" is not offered for a record outside the work folder. */
export const RESUME_WORK_DIR_ONLY = '작업 폴더의 기록만 이어서 생성할 수 있습니다. 다른 폴더의 기록은 "이 설정 다시 쓰기"로 새로 만드세요.';

/**
 * The service always writes into the work folder, so resuming a record of another gallery folder would make every
 * image again (paid) in a new place. Null when `folder` (GalleryItem.folder) is the work folder's gallery root.
 */
export function resumeBlocked(folder: string, folders: FolderView[]): string | null {
  return folders.some((f) => f.isWorkDir && samePath(f.galleryRoot, folder)) ? null : RESUME_WORK_DIR_ONLY;
}

// ---- history (right panel) ----

export interface HistoryGroup {
  key: string;
  topic: string;
  folder: string;
  prompt: string;
  /** The user's request before "영어로 다듬기", shown as the title when there is one. */
  original: string | null;
  model: string;
  /** Newest record time in the group. */
  modified: number;
  /** In image number order. */
  items: GalleryItem[];
  counts: { done: number; failed: number; other: number };
  /** Up to 4 finished images for the thumbnail grid. */
  thumbs: string[];
}

function isDone(item: GalleryItem): boolean {
  return item.status === 'complete' || item.status === 'legacy';
}

/** Records grouped by gallery folder and topic, newest group first (at most `limit`). */
export function historyGroups(items: GalleryItem[], limit = 40): HistoryGroup[] {
  const groups = new Map<string, GalleryItem[]>();
  for (const item of items) {
    const key = `${item.folder.toLowerCase()}|${item.topic}`;
    const list = groups.get(key);
    if (list) list.push(item);
    else groups.set(key, [item]);
  }
  const order = (i: GalleryItem) => imageNumber(i.imagePath) ?? Number.MAX_SAFE_INTEGER;
  const out: HistoryGroup[] = [];
  for (const [key, list] of groups) {
    const sorted = [...list].sort((a, b) => order(a) - order(b) || a.imagePath.localeCompare(b.imagePath));
    const counts = { done: 0, failed: 0, other: 0 };
    for (const i of sorted) {
      if (isDone(i)) counts.done++;
      else if (i.status === 'failed') counts.failed++;
      else counts.other++;
    }
    out.push({
      key,
      topic: sorted[0].topic,
      folder: sorted[0].folder,
      prompt: sorted[0].prompt,
      original: sorted[0].originalPrompt ?? null,
      model: sorted[0].imageModel,
      modified: Math.max(...sorted.map((i) => i.modified)),
      items: sorted,
      counts,
      thumbs: sorted.filter(isDone).slice(0, 4).map((i) => i.imagePath),
    });
  }
  out.sort((a, b) => b.modified - a.modified);
  return out.slice(0, limit);
}

// ---- result cards ----

export type DisplayStatus = CardStatus | 'cancelled' | 'missing' | 'pending';

export interface DisplayCard {
  key: string;
  /** "#3", or "" for a single unnumbered image. */
  label: string;
  status: DisplayStatus;
  /** Image to show; only for finished images. */
  imagePath: string | null;
  /** Retry line, or the Korean failure cause, else "". */
  text: string;
  /** Raw engine/provider message of a failure (shown behind "자세히"), else "". */
  detail: string;
  transparent: boolean;
  elapsedSec?: number;
}

/**
 * A card of the running (or last) run. While `cancelling`, an image that stops without a category (imageFailed
 * carries none; the finished event adds it) is shown as cancelled, not as a red failure.
 */
export function runCard(card: Card, transparent = false, cancelling = false): DisplayCard {
  const key = `run-${card.index}`;
  const label = `#${card.index}`;
  if (card.status === 'failed' && (card.category === 'cancelled' || (cancelling && !card.category))) {
    return { key, label, status: 'cancelled', imagePath: null, text: '', detail: '', transparent };
  }
  if (card.status === 'failed') {
    const text = card.category ? categoryLabel(card.category) : '';
    return { key, label, status: 'failed', imagePath: null, text, detail: card.reason ?? '', transparent, elapsedSec: card.elapsedSec };
  }
  const finished = card.status === 'done' || card.status === 'skipped';
  return {
    key,
    label,
    status: card.status,
    imagePath: finished && card.output ? card.output : null,
    text: card.status === 'skipped' ? '이미 만든 이미지(이어서 생성)' : cardText(card),
    detail: '',
    transparent,
    elapsedSec: card.elapsedSec,
  };
}

export interface CardTally {
  done: number;
  running: number;
  retrying: number;
  /** Failures other than cancel. */
  failed: number;
  cancelled: number;
  total: number;
}

/** Counters over the shown cards (skipped counts as done, like run.ts counts). */
export function tallyCards(cards: DisplayCard[]): CardTally {
  const n: CardTally = { done: 0, running: 0, retrying: 0, failed: 0, cancelled: 0, total: cards.length };
  for (const c of cards) {
    if (c.status === 'done' || c.status === 'skipped') n.done++;
    else if (c.status === 'running') n.running++;
    else if (c.status === 'retrying') n.retrying++;
    else if (c.status === 'failed') n.failed++;
    else if (c.status === 'cancelled') n.cancelled++;
  }
  return n;
}

export type Tone = 'ok' | 'run' | 'warn' | 'fail' | 'neutral';

/** Live counters of a running run; zero ones are left out. */
export function runPills(t: CardTally): { key: string; label: string; tone: Tone }[] {
  const all: { key: string; label: string; tone: Tone; n: number }[] = [
    { key: 'done', label: `완료 ${t.done}`, tone: 'ok', n: t.done },
    { key: 'running', label: `진행 ${t.running}`, tone: 'run', n: t.running },
    { key: 'retrying', label: `재시도 ${t.retrying}`, tone: 'warn', n: t.retrying },
    { key: 'failed', label: `실패 ${t.failed}`, tone: 'fail', n: t.failed },
    { key: 'cancelled', label: `취소 ${t.cancelled}`, tone: 'neutral', n: t.cancelled },
  ];
  return all.filter((p) => p.n > 0).map(({ key, label, tone }) => ({ key, label, tone }));
}

/** One summary line of a finished run, e.g. "5장 완료 · 1장 실패 · 3.6초" or "취소됨 · 완료 0/6 · 2.1초". */
export function runOutcome(t: CardTally, elapsedSec: number | null, cancelled: boolean): { line: string; tone: 'ok' | 'warn' | 'neutral' } {
  const parts = cancelled ? ['취소됨', `완료 ${t.done}/${t.total}`] : [`${t.done}장 완료`];
  if (t.failed > 0) parts.push(`${t.failed}장 실패`);
  if (elapsedSec !== null) parts.push(formatElapsed(elapsedSec));
  return { line: parts.join(' · '), tone: cancelled ? 'neutral' : t.failed > 0 ? 'warn' : 'ok' };
}

/** Progress bar: share of finished images while running; after a cancel only what was made, in a neutral colour. */
export function progressView(t: CardTally, running: boolean, cancelled: boolean): { fraction: number; tone: 'accent' | 'warn' | 'neutral' } {
  if (t.total <= 0) return { fraction: 0, tone: 'accent' };
  if (!running && cancelled) return { fraction: t.done / t.total, tone: 'neutral' };
  const fraction = (t.done + t.failed + t.cancelled) / t.total;
  return { fraction, tone: !running && t.failed > 0 ? 'warn' : 'accent' };
}

/** "실패한 장 다시 만들기": the same settings resumed on the run's topic, so finished images are kept. */
export function retryInput(input: GenerateInput, topic: string): GenerateInput {
  return { ...input, references: [...input.references], topic, resume: true };
}

/** A card of a gallery record (history view). */
export function itemCard(item: GalleryItem): DisplayCard {
  const n = imageNumber(item.imagePath);
  const base = { key: item.recordPath, label: n === null ? '' : `#${n}`, transparent: item.background === 'transparent', detail: '' };
  switch (item.status) {
    case 'complete':
    case 'legacy':
      return { ...base, status: 'done', imagePath: item.imagePath, text: '' };
    case 'failed':
      return { ...base, status: 'failed', imagePath: null, text: '', detail: item.error ?? '' };
    case 'pending':
      return { ...base, status: 'pending', imagePath: null, text: '끝나지 않은 기록' };
    default:
      return { ...base, status: 'missing', imagePath: null, text: '기록은 있지만 파일 없음' };
  }
}
