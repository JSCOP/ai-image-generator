// Gallery screen (pure): status groups and badges, folder/status/search filters, day groups and the detail rows of a record.

import { dayNumber, formatRelativeTime, formatSize } from './format';
import { samePath } from './generateForm';
import type { GalleryItem, GalleryStatus } from './types';

/** Status filter choices: legacy (no status, Python) counts as finished; pending shows only under "all". */
export type StatusFilter = 'all' | 'done' | 'failed' | 'missing';

export const STATUS_FILTERS: { value: StatusFilter; label: string }[] = [
  { value: 'all', label: '전체' },
  { value: 'done', label: '완료' },
  { value: 'failed', label: '실패' },
  { value: 'missing', label: '파일 없음' },
];

export interface GalleryFilter {
  /** Gallery root (GalleryItem.folder) or null for every folder. */
  folder: string | null;
  status: StatusFilter;
  query: string;
}

export type StatusGroup = 'done' | 'failed' | 'missing' | 'pending';

export function statusGroup(status: GalleryStatus): StatusGroup {
  switch (status) {
    case 'complete':
    case 'legacy':
      return 'done';
    case 'failed':
      return 'failed';
    case 'pending':
      return 'pending';
    default:
      return 'missing';
  }
}

export interface StatusBadge {
  label: string;
  tone: 'neutral' | 'ok' | 'warn' | 'error';
  /** Longer text on hover. */
  title: string;
}

export function statusBadge(item: GalleryItem): StatusBadge {
  switch (item.status) {
    case 'complete':
      return { label: '완료', tone: 'ok', title: '이미지와 기록이 모두 있습니다.' };
    case 'legacy':
      return { label: '완료', tone: 'ok', title: '상태 값이 없는 옛 기록(이전 Python 도구)입니다.' };
    case 'failed':
      return { label: '실패', tone: 'error', title: item.error || '실패한 기록입니다.' };
    case 'pending':
      return { label: '끝나지 않음', tone: 'warn', title: '생성이 끝나기 전에 멈춘 기록입니다. 이어서 생성하면 마무리됩니다.' };
    default:
      return { label: '파일 없음', tone: 'neutral', title: '기록은 있지만 이미지 파일이 없습니다.' };
  }
}

function matchesStatus(item: GalleryItem, status: StatusFilter): boolean {
  return status === 'all' || statusGroup(item.status) === status;
}

function matchesFolder(item: GalleryItem, folder: string | null): boolean {
  return folder === null || samePath(item.folder, folder);
}

function searchWords(query: string): string[] {
  return query.toLowerCase().split(/\s+/).filter(Boolean);
}

/** Every word of the search appears in the prompt or a model name (case ignored). */
function matchesWords(item: GalleryItem, words: string[]): boolean {
  if (words.length === 0) return true;
  const text = [item.prompt, item.originalPrompt ?? '', item.imageModel, item.responseModel ?? ''].join('\n').toLowerCase();
  return words.every((w) => text.includes(w));
}

export function filterItems(items: GalleryItem[], filter: GalleryFilter): GalleryItem[] {
  const words = searchWords(filter.query);
  return items.filter((i) => matchesFolder(i, filter.folder) && matchesStatus(i, filter.status) && matchesWords(i, words));
}

export type StatusCounts = Record<StatusFilter, number>;

/** How many records each status choice would show. */
export function statusCounts(items: GalleryItem[]): StatusCounts {
  const n: StatusCounts = { all: items.length, done: 0, failed: 0, missing: 0 };
  for (const i of items) {
    const g = statusGroup(i.status);
    if (g !== 'pending') n[g]++;
  }
  return n;
}

/** "오늘", "어제", "10월 3일", or "2025년 4월 23일" for another year (local time). */
export function dateLabel(epochSec: number, now: Date = new Date()): string {
  const t = new Date(epochSec * 1000);
  const diff = dayNumber(now) - dayNumber(t);
  if (diff === 0) return '오늘';
  if (diff === 1) return '어제';
  const date = `${t.getMonth() + 1}월 ${t.getDate()}일`;
  return t.getFullYear() === now.getFullYear() ? date : `${t.getFullYear()}년 ${date}`;
}

export interface DateGroup {
  key: string;
  label: string;
  /** Newest first. */
  items: GalleryItem[];
}

/** Records by local day of their record time, newest day first. */
export function groupByDate(items: GalleryItem[], now: Date = new Date()): DateGroup[] {
  const days = new Map<number, GalleryItem[]>();
  for (const item of items) {
    const day = dayNumber(new Date(item.modified * 1000));
    const list = days.get(day);
    if (list) list.push(item);
    else days.set(day, [item]);
  }
  return [...days.entries()]
    .sort((a, b) => b[0] - a[0])
    .map(([day, list]) => {
      const sorted = [...list].sort((a, b) => b.modified - a.modified || a.recordPath.localeCompare(b.recordPath));
      return { key: `d${day}`, label: dateLabel(sorted[0].modified, now), items: sorted };
    });
}

/** The first `limit` records of the day groups (so a long gallery renders in steps). */
export function takeItems(groups: DateGroup[], limit: number): DateGroup[] {
  const out: DateGroup[] = [];
  let left = limit;
  for (const g of groups) {
    if (left <= 0) break;
    out.push(g.items.length <= left ? g : { ...g, items: g.items.slice(0, left) });
    left -= g.items.length;
  }
  return out;
}

/** Records of the same run: same gallery folder and topic, in list order. */
export function sameTopic(items: GalleryItem[], of: GalleryItem): GalleryItem[] {
  return items.filter((i) => i.topic === of.topic && samePath(i.folder, of.folder));
}

/** Last folder name of a path ("D:\\projects\\brand-kit" -> "brand-kit"); a drive root stays as is. */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= 1) return path;
  return parts[parts.length - 1];
}

export interface DetailRow {
  label: string;
  value: string;
}

const QUALITY_LABELS: Record<string, string> = { low: '낮음', medium: '보통', high: '높음' };
const BACKGROUND_LABELS: Record<string, string> = { transparent: '투명', opaque: '불투명', auto: '자동' };

/** Main values of a record for the detail panel; values the record lacks are left out. */
export function detailRows(item: GalleryItem, now: Date = new Date()): DetailRow[] {
  const rows: DetailRow[] = [{ label: '상태', value: statusBadge(item).label }];
  if (item.status === 'failed') rows.push({ label: '실패 원인', value: item.error || '알 수 없음' });
  rows.push({ label: '모델', value: item.imageModel });
  if (item.responseModel && item.responseModel !== item.imageModel) rows.push({ label: '응답 모델', value: item.responseModel });
  const size =
    item.providerSize && item.savedSize && item.providerSize !== item.savedSize
      ? `${formatSize(item.providerSize)} → ${formatSize(item.savedSize)}`
      : formatSize(item.savedSize ?? item.size);
  rows.push({ label: '크기', value: size });
  if (item.quality) rows.push({ label: '품질', value: QUALITY_LABELS[item.quality] ?? item.quality });
  if (item.background) {
    let value = BACKGROUND_LABELS[item.background] ?? item.background;
    if (item.transparentRatio != null) value += ` (완전 투명 ${Number((item.transparentRatio * 100).toFixed(1))}%)`;
    rows.push({ label: '배경', value });
  }
  if (item.references.length > 0) rows.push({ label: '참조 이미지', value: `${item.references.length}장` });
  if (item.requestId) rows.push({ label: '요청 ID', value: item.requestId });
  rows.push({ label: '만든 때', value: formatRelativeTime(item.modified, now) });
  rows.push({ label: '주제', value: item.topic });
  return rows;
}
