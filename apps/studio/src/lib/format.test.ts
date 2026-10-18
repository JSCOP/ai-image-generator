import { describe, expect, it } from 'vitest';
import { categoryLabel, formatBytes, formatElapsed, formatRelativeTime, formatSize, oneLine, truncateEnd, truncateMiddle } from './format';
import type { Category } from './types';

/** Local time -> epoch seconds. */
function at(y: number, mo: number, d: number, h = 0, mi = 0, s = 0): number {
  return new Date(y, mo - 1, d, h, mi, s).getTime() / 1000;
}

describe('formatSize', () => {
  it('shows the size with a multiplication sign', () => {
    expect(formatSize('1920x1080')).toBe('1920×1080');
  });

  it('leaves other text as is', () => {
    expect(formatSize('auto')).toBe('auto');
    expect(formatSize('')).toBe('');
  });
});

describe('formatRelativeTime', () => {
  const now = new Date(2026, 9, 18, 14, 30); // 2026-10-18 14:30 local

  it('says 오늘 with the time for today', () => {
    expect(formatRelativeTime(at(2026, 10, 18, 14, 5), now)).toBe('오늘 14:05');
  });

  it('counts local midnight as today', () => {
    expect(formatRelativeTime(at(2026, 10, 18, 0, 0, 0), now)).toBe('오늘 00:00');
  });

  it('says 어제 for the day before, up to the last second', () => {
    expect(formatRelativeTime(at(2026, 10, 17, 23, 59, 59), now)).toBe('어제 23:59');
    expect(formatRelativeTime(at(2026, 10, 17, 9, 10), now)).toBe('어제 09:10');
  });

  it('shows month and day for older dates of this year', () => {
    expect(formatRelativeTime(at(2026, 10, 16, 23, 59), now)).toBe('10월 16일');
    expect(formatRelativeTime(at(2026, 10, 3, 8, 0), now)).toBe('10월 3일');
  });

  it('adds the year for another year', () => {
    expect(formatRelativeTime(at(2025, 4, 23, 8, 0), now)).toBe('2025년 4월 23일');
  });

  it('treats yesterday across a month boundary', () => {
    const first = new Date(2026, 10, 1, 9, 0); // 11월 1일
    expect(formatRelativeTime(at(2026, 10, 31, 22, 0), first)).toBe('어제 22:00');
  });

  it('shows a date for a future day (clock skew) instead of a wrong word', () => {
    expect(formatRelativeTime(at(2026, 10, 19, 1, 0), now)).toBe('10월 19일');
  });

  it('returns an empty text for a missing time', () => {
    expect(formatRelativeTime(Number.NaN, now)).toBe('');
  });
});

describe('truncateMiddle / truncateEnd', () => {
  it('keeps short text unchanged', () => {
    expect(truncateMiddle('짧은 글', 10)).toBe('짧은 글');
    expect(truncateEnd('짧은 글', 10)).toBe('짧은 글');
  });

  it('cuts the middle with an ellipsis and keeps the limit', () => {
    const out = truncateMiddle('C:/Users/js/Pictures/Image Studio/ImageGallery/output/topic/image.png', 21);
    expect(Array.from(out)).toHaveLength(21);
    expect(out.startsWith('C:/Users/')).toBe(true);
    expect(out.endsWith('image.png')).toBe(true);
    expect(out).toContain('…');
  });

  it('never splits a surrogate pair or an emoji sequence in long Korean text', () => {
    const text = '아늑한 독서 공간 😀👨‍👩‍👧 오후 햇살 🌅 창가에 앉은 고양이 🐈‍⬛ 그리고 따뜻한 차 한 잔 ☕️ '.repeat(8);
    for (let max = 2; max < 60; max++) {
      for (const out of [truncateMiddle(text, max), truncateEnd(text, max)]) {
        // A lone surrogate would show as a broken glyph.
        expect(out).not.toMatch(/[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/);
        // The ZWJ family stays whole or is gone entirely.
        expect(out.includes('👨‍👩') === out.includes('👨‍👩‍👧')).toBe(true);
        expect(out).not.toMatch(/\u200D$/);
      }
    }
  });

  it('puts the ellipsis at the end for truncateEnd', () => {
    expect(truncateEnd('가나다라마바사', 4)).toBe('가나다…');
  });

  it('turns line breaks and runs of spaces into one space', () => {
    expect(oneLine('첫 줄\n\n둘째   줄\t끝 ')).toBe('첫 줄 둘째 줄 끝');
  });
});

describe('formatBytes / formatElapsed', () => {
  it('uses B, KB and MB', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2 KB');
    expect(formatBytes(10 * 1024 * 1024)).toBe('10 MB');
    expect(formatBytes(1.5 * 1024 * 1024)).toBe('1.5 MB');
  });

  it('shows seconds, then minutes', () => {
    expect(formatElapsed(4.26)).toBe('4.3초');
    expect(formatElapsed(75)).toBe('1분 15초');
  });
});

describe('categoryLabel', () => {
  it('gives every category a Korean label', () => {
    const all: Category[] = [
      'network',
      'dns',
      'timeout',
      'tls',
      'auth',
      'permission',
      'quota',
      'rate_limit',
      'conflict',
      'cancelled',
      'not_found',
      'redirect',
      'unsupported',
      'invalid_request',
      'provider',
      'internal',
    ];
    for (const c of all) expect(categoryLabel(c)).toMatch(/[가-힣]/);
    expect(categoryLabel('dns')).toBe('주소를 찾을 수 없음(오타 또는 Tailscale 꺼짐)');
    expect(categoryLabel('rate_limit')).toBe('요청이 너무 많음(잠시 후 재시도)');
    expect(categoryLabel('not_found')).toBe('주소 경로 오류(/v1 확인)');
  });

  it('falls back for an unknown value', () => {
    expect(categoryLabel('something_new' as Category)).toBe('알 수 없는 오류');
  });
});
