import { describe, expect, it } from 'vitest';
import {
  dateLabel,
  detailRows,
  filterItems,
  folderName,
  groupByDate,
  sameTopic,
  statusBadge,
  statusCounts,
  statusGroup,
  takeItems,
  type GalleryFilter,
} from './galleryView';
import type { GalleryItem } from './types';

/** Local time -> epoch seconds. */
function at(y: number, mo: number, d: number, h = 0, mi = 0, s = 0): number {
  return new Date(y, mo - 1, d, h, mi, s).getTime() / 1000;
}

const NOW = new Date(2026, 9, 19, 15, 30, 0); // 2026-10-19 15:30 local
const WORK = 'C:\\Users\\js\\Pictures\\Image Studio\\ImageGallery';
const PROJECT = 'D:\\projects\\brand-kit\\ImageGallery';

function item(over: Partial<GalleryItem> = {}): GalleryItem {
  const topic = over.topic ?? 'mug-20261019-101010';
  const name = over.imagePath ?? `${WORK}\\output\\${topic}\\${topic}_001.png`;
  return {
    imagePath: name,
    recordPath: name.replace('\\output\\', '\\metadata\\').replace(/\.png$/, '.json'),
    folder: WORK,
    topic,
    status: 'complete',
    prompt: 'product shot, ceramic mug',
    imageModel: 'gpt-image-2.5',
    responseModel: null,
    size: '1024x1024',
    background: 'auto',
    transparentRatio: null,
    error: null,
    modified: at(2026, 10, 19, 10, 10),
    quality: 'medium',
    references: [],
    providerSize: null,
    savedSize: null,
    requestId: null,
    ...over,
  };
}

const ALL: GalleryFilter = { folder: null, status: 'all', query: '' };

describe('statusGroup / statusBadge', () => {
  it('counts legacy (no status, Python) as finished and keeps the rest apart', () => {
    expect(statusGroup('complete')).toBe('done');
    expect(statusGroup('legacy')).toBe('done');
    expect(statusGroup('failed')).toBe('failed');
    expect(statusGroup('missing')).toBe('missing');
    expect(statusGroup('pending')).toBe('pending');
  });

  it('labels each status in Korean, failed with its cause', () => {
    expect(statusBadge(item()).label).toBe('완료');
    expect(statusBadge(item({ status: 'legacy' })).label).toBe('완료');
    expect(statusBadge(item({ status: 'legacy' })).title).toContain('옛 기록');
    expect(statusBadge(item({ status: 'missing' }))).toMatchObject({ label: '파일 없음', tone: 'neutral' });
    expect(statusBadge(item({ status: 'pending' }))).toMatchObject({ label: '끝나지 않음', tone: 'warn' });
    const failed = statusBadge(item({ status: 'failed', error: '사용 한도 초과: billing_hard_limit_reached' }));
    expect(failed).toMatchObject({ label: '실패', tone: 'error' });
    expect(failed.title).toBe('사용 한도 초과: billing_hard_limit_reached');
  });
});

describe('filterItems', () => {
  const items = [
    item({ topic: 'a', status: 'complete', prompt: 'Cozy Reading Nook', imageModel: 'gpt-image-2.5' }),
    item({ topic: 'b', status: 'failed', prompt: '공항 대시보드', imageModel: 'gpt-image-2', error: 'quota' }),
    item({ topic: 'c', status: 'missing', prompt: 'mug', imageModel: 'gemini-3.1-flash-image' }),
    item({ topic: 'd', status: 'legacy', prompt: 'old run', folder: PROJECT, imageModel: 'gpt-image-1.5' }),
    item({ topic: 'e', status: 'pending', prompt: 'unfinished', folder: PROJECT, imageModel: 'gpt-image-2.5', responseModel: 'gpt-image-2.5-2026-10-01' }),
  ];
  const topics = (f: Partial<GalleryFilter>) => filterItems(items, { ...ALL, ...f }).map((i) => i.topic);

  it('shows everything with no filter', () => {
    expect(topics({})).toEqual(['a', 'b', 'c', 'd', 'e']);
  });

  it('filters by status group (pending only under all)', () => {
    expect(topics({ status: 'done' })).toEqual(['a', 'd']);
    expect(topics({ status: 'failed' })).toEqual(['b']);
    expect(topics({ status: 'missing' })).toEqual(['c']);
  });

  it('filters by folder ignoring case, slash direction and a trailing separator', () => {
    expect(topics({ folder: 'd:/projects/BRAND-KIT/imagegallery/' })).toEqual(['d', 'e']);
    expect(topics({ folder: WORK })).toEqual(['a', 'b', 'c']);
  });

  it('searches prompt and models ignoring case', () => {
    expect(topics({ query: 'cozy reading' })).toEqual(['a']);
    expect(topics({ query: 'COZY' })).toEqual(['a']);
    expect(topics({ query: '대시보드' })).toEqual(['b']);
    expect(topics({ query: 'GEMINI' })).toEqual(['c']);
    expect(topics({ query: '2026-10-01' })).toEqual(['e']);
    expect(topics({ query: '   ' })).toEqual(['a', 'b', 'c', 'd', 'e']);
  });

  it('needs every word of the search', () => {
    expect(topics({ query: 'gpt-image-2.5 nook' })).toEqual(['a']);
    expect(topics({ query: 'nook mug' })).toEqual([]);
  });

  it('combines folder, status and search', () => {
    expect(topics({ folder: PROJECT, status: 'done' })).toEqual(['d']);
    expect(topics({ folder: PROJECT, status: 'done', query: 'unfinished' })).toEqual([]);
    expect(topics({ folder: WORK, status: 'failed', query: 'gpt-image-2' })).toEqual(['b']);
  });
});

describe('statusCounts', () => {
  it('counts each filter choice within the folder and search', () => {
    const items = [item({ status: 'complete' }), item({ status: 'legacy' }), item({ status: 'failed' }), item({ status: 'missing' }), item({ status: 'pending' })];
    expect(statusCounts(items)).toEqual({ all: 5, done: 2, failed: 1, missing: 1 });
    expect(statusCounts([])).toEqual({ all: 0, done: 0, failed: 0, missing: 0 });
  });
});

describe('dateLabel / groupByDate', () => {
  it('names today, yesterday and older days', () => {
    expect(dateLabel(at(2026, 10, 19, 0, 0, 1), NOW)).toBe('오늘');
    expect(dateLabel(at(2026, 10, 18, 23, 59, 59), NOW)).toBe('어제');
    expect(dateLabel(at(2026, 10, 17, 12), NOW)).toBe('10월 17일');
    expect(dateLabel(at(2025, 4, 23, 12), NOW)).toBe('2025년 4월 23일');
  });

  it('groups by local day, newest day first and newest item first', () => {
    const items = [
      item({ topic: 'old', modified: at(2026, 10, 3, 9) }),
      item({ topic: 'today-early', modified: at(2026, 10, 19, 8) }),
      item({ topic: 'yesterday', modified: at(2026, 10, 18, 22) }),
      item({ topic: 'today-late', modified: at(2026, 10, 19, 14) }),
      item({ topic: 'old2', modified: at(2026, 10, 3, 18) }),
    ];
    const groups = groupByDate(items, NOW);
    expect(groups.map((g) => g.label)).toEqual(['오늘', '어제', '10월 3일']);
    expect(groups[0].items.map((i) => i.topic)).toEqual(['today-late', 'today-early']);
    expect(groups[2].items.map((i) => i.topic)).toEqual(['old2', 'old']);
    expect(new Set(groups.map((g) => g.key)).size).toBe(3);
  });

  it('gives no group for no items', () => {
    expect(groupByDate([], NOW)).toEqual([]);
  });
});

describe('takeItems', () => {
  it('keeps whole day groups until the limit and cuts the last one', () => {
    const items = [
      ...[1, 2, 3].map((n) => item({ topic: `t${n}`, modified: at(2026, 10, 19, n) })),
      ...[1, 2, 3].map((n) => item({ topic: `y${n}`, modified: at(2026, 10, 18, n) })),
    ];
    const groups = groupByDate(items, NOW);
    const four = takeItems(groups, 4);
    expect(four.map((g) => g.items.length)).toEqual([3, 1]);
    expect(takeItems(groups, 3).map((g) => g.items.length)).toEqual([3]);
    expect(takeItems(groups, 100).map((g) => g.items.length)).toEqual([3, 3]);
  });
});

describe('sameTopic', () => {
  it('finds the records of one run (same folder and topic)', () => {
    const a1 = item({ topic: 'a', imagePath: `${WORK}\\output\\a\\a_001.png` });
    const a2 = item({ topic: 'a', imagePath: `${WORK}\\output\\a\\a_002.png` });
    const other = item({ topic: 'a', folder: PROJECT, imagePath: `${PROJECT}\\output\\a\\a_001.png` });
    const b = item({ topic: 'b' });
    expect(sameTopic([a1, other, b, a2], a2).map((i) => i.imagePath)).toEqual([a1.imagePath, a2.imagePath]);
  });
});

describe('folderName', () => {
  it('shows the last folder of a path', () => {
    expect(folderName('C:\\Users\\js\\Pictures\\Image Studio')).toBe('Image Studio');
    expect(folderName('D:/projects/brand-kit/')).toBe('brand-kit');
    expect(folderName('E:\\')).toBe('E:\\');
  });
});

describe('detailRows', () => {
  it('lists the main record values in Korean', () => {
    const rows = detailRows(
      item({
        status: 'complete',
        responseModel: 'gpt-image-2.5-2026-10-01',
        providerSize: '1448x1086',
        savedSize: '1024x1024',
        quality: 'high',
        background: 'transparent',
        transparentRatio: 0.625,
        requestId: 'req_123',
        references: ['D:\\refs\\a.png'],
      }),
      NOW,
    );
    const get = (label: string) => rows.find((r) => r.label === label)?.value;
    expect(get('상태')).toBe('완료');
    expect(get('모델')).toBe('gpt-image-2.5');
    expect(get('응답 모델')).toBe('gpt-image-2.5-2026-10-01');
    expect(get('크기')).toBe('1448×1086 → 1024×1024');
    expect(get('품질')).toBe('높음');
    expect(get('배경')).toBe('투명 (완전 투명 62.5%)');
    expect(get('요청 ID')).toBe('req_123');
    expect(get('참조 이미지')).toBe('1장');
    expect(get('만든 때')).toBe('오늘 10:10');
    expect(get('주제')).toBe('mug-20261019-101010');
  });

  it('leaves out values the record does not have and shows the failure cause', () => {
    const rows = detailRows(item({ status: 'failed', error: '사용 한도 초과', quality: null, requestId: null, background: null }), NOW);
    const labels = rows.map((r) => r.label);
    expect(labels).not.toContain('응답 모델');
    expect(labels).not.toContain('요청 ID');
    expect(labels).not.toContain('품질');
    expect(labels).not.toContain('참조 이미지');
    expect(rows.find((r) => r.label === '실패 원인')?.value).toBe('사용 한도 초과');
    expect(rows.find((r) => r.label === '크기')?.value).toBe('1024×1024');
  });
});
