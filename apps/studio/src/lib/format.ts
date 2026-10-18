// Pure formatters for screen text (sizes, times, lengths, error categories).

import type { Category } from './types';

/** "1920x1080" -> "1920×1080" (any other text is returned as is). */
export function formatSize(size: string): string {
  return size.replace(/^(\d+)\s*[xX×]\s*(\d+)$/, '$1×$2');
}

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

/** Local calendar day number (days since the epoch). */
export function dayNumber(d: Date): number {
  // Local calendar day, independent of DST hour shifts.
  return Math.round(new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() / 86_400_000);
}

/** "오늘 14:05", "어제 09:10", "10월 3일", or "2025년 4월 23일" for another year (local time). */
export function formatRelativeTime(epochSec: number, now: Date = new Date()): string {
  if (!Number.isFinite(epochSec)) return '';
  const t = new Date(epochSec * 1000);
  const diff = dayNumber(now) - dayNumber(t);
  const clock = `${pad2(t.getHours())}:${pad2(t.getMinutes())}`;
  if (diff === 0) return `오늘 ${clock}`;
  if (diff === 1) return `어제 ${clock}`;
  const date = `${t.getMonth() + 1}월 ${t.getDate()}일`;
  return t.getFullYear() === now.getFullYear() ? date : `${t.getFullYear()}년 ${date}`;
}

/** Line breaks and runs of white space become one space. */
export function oneLine(text: string): string {
  return text.replace(/\s+/g, ' ').trim();
}

const segmenter = typeof Intl !== 'undefined' && 'Segmenter' in Intl ? new Intl.Segmenter('ko', { granularity: 'grapheme' }) : null;

/** User-visible characters, so emoji (surrogate pairs, ZWJ sequences) are never split. */
function graphemes(text: string): string[] {
  if (segmenter) return Array.from(segmenter.segment(text), (s) => s.segment);
  return Array.from(text);
}

/** At most `max` visible characters, the end replaced by "…". */
export function truncateEnd(text: string, max: number): string {
  const g = graphemes(text);
  if (g.length <= max) return text;
  if (max <= 1) return '…';
  return g.slice(0, max - 1).join('') + '…';
}

/** At most `max` visible characters, the middle replaced by "…" (for paths: the file name stays visible). */
export function truncateMiddle(text: string, max: number): string {
  const g = graphemes(text);
  if (g.length <= max) return text;
  if (max <= 1) return '…';
  const keep = max - 1;
  const tail = Math.floor(keep / 2);
  const head = keep - tail;
  return g.slice(0, head).join('') + '…' + (tail > 0 ? g.slice(g.length - tail).join('') : '');
}

/** 512 B, 2 KB, 1.5 MB. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${Number(value.toFixed(1))} ${units[unit]}`;
}

/** 4.3초, 1분 15초. */
export function formatElapsed(sec: number): string {
  if (sec < 60) return `${Number(sec.toFixed(1))}초`;
  const whole = Math.round(sec);
  return `${Math.floor(whole / 60)}분 ${whole % 60}초`;
}

const CATEGORY_LABELS: Record<Category, string> = {
  network: '연결 끊김',
  dns: '주소를 찾을 수 없음(오타 또는 Tailscale 꺼짐)',
  timeout: '시간 초과',
  tls: '인증서 문제',
  auth: '키가 거부됨',
  permission: '권한 없음',
  quota: '사용 한도 초과',
  rate_limit: '요청이 너무 많음(잠시 후 재시도)',
  conflict: '다른 작업과 겹침',
  cancelled: '취소됨',
  not_found: '주소 경로 오류(/v1 확인)',
  redirect: '다른 주소로 이동 요구(따라가지 않음)',
  unsupported: '이 연결/모델에서 지원하지 않음',
  invalid_request: '요청 오류',
  provider: '제공자 오류',
  internal: '앱 내부 오류',
};

/** Korean meaning of an error category. */
export function categoryLabel(category: Category): string {
  return CATEGORY_LABELS[category] ?? '알 수 없는 오류';
}
