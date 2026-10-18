import { describe, expect, it } from 'vitest';
import {
  addReferences,
  capabilities,
  connectionChoices,
  connectionState,
  CUSTOM_SIZE,
  defaultConnection,
  defaultModel,
  emptyForm,
  formFromItem,
  historyGroups,
  imageNumber,
  itemCard,
  MAX_COUNT,
  parseSize,
  progressView,
  RESUME_WORK_DIR_ONLY,
  retryInput,
  resumeBlocked,
  resumeFrom,
  runCard,
  runOutcome,
  runPills,
  samePath,
  sizeOf,
  splitPrompt,
  tallyCards,
  toInput,
  type GenerateForm,
} from './generateForm';
import type { ConnectionsView, FolderView, GalleryItem, ModelView, ProfileView, ReferenceView } from './types';

const MB = 1024 * 1024;

const models: ModelView[] = [
  { id: 'gpt-image-2.5', provider: 'OpenAI', references: true, maxReferences: 4, transparency: false },
  { id: 'gpt-image-2.5-sunburst', provider: 'OpenAI', references: true, maxReferences: 4, transparency: true },
  { id: 'grok-imagine-image', provider: 'xAI Grok', references: true, maxReferences: 1, transparency: false },
  { id: 'grok-imagine-image-2.0', provider: 'xAI Grok', references: false, maxReferences: 0, transparency: false },
];

function profile(name: string, isDefault = false): ProfileView {
  return {
    id: `p-${name}`,
    name,
    kind: 'cliproxy',
    kindLabel: 'CLIProxyAPI',
    baseUrl: 'http://127.0.0.1:8317/v1',
    concurrency: null,
    isDefault,
    keyConfigured: true,
    warning: null,
    lastGeneration: null,
  };
}

function ref(name: string, bytes = 2 * MB): ReferenceView {
  return { path: `D:\\refs\\${name}`, name, bytes, width: 1024, height: 1024 };
}

function form(change: Partial<GenerateForm> = {}): GenerateForm {
  return { ...emptyForm(), connection: 'local', prompt: '아늑한 독서 공간', model: 'gpt-image-2.5', ...change };
}

function item(change: Partial<GalleryItem> = {}): GalleryItem {
  return {
    imagePath: 'C:\\w\\ImageGallery\\output\\t\\t_1.png',
    recordPath: 'C:\\w\\ImageGallery\\metadata\\t\\t_1.png.json',
    folder: 'C:\\w\\ImageGallery',
    topic: 't',
    status: 'complete',
    prompt: '고양이',
    imageModel: 'gpt-image-2.5',
    responseModel: null,
    size: '1024x1024',
    background: 'auto',
    transparentRatio: null,
    error: null,
    modified: 1000,
    quality: 'low',
    references: [],
    providerSize: null,
    savedSize: null,
    requestId: null,
    ...change,
  };
}

describe('toInput', () => {
  it('turns the form into the GenerateInput the service takes', () => {
    const result = toInput(
      form({ avoid: '  글자  ', size: '1920x1080', quality: 'medium', count: 4, references: [ref('a.png')], transparent: false }),
      models,
    );
    expect(result).toEqual({
      ok: true,
      input: {
        profile: 'local',
        prompt: '아늑한 독서 공간',
        avoid: '글자',
        imageModel: 'gpt-image-2.5',
        size: '1920x1080',
        quality: 'medium',
        count: 4,
        references: ['D:\\refs\\a.png'],
        background: 'auto',
        topic: null,
        resume: false,
      },
    });
  });

  it('sends null as profile for the environment connection and transparent when asked', () => {
    const result = toInput(form({ connection: 'env', model: 'gpt-image-2.5-sunburst', transparent: true }), models);
    expect(result.ok && result.input.profile).toBe(null);
    expect(result.ok && result.input.background).toBe('transparent');
  });

  it('uses the custom size when chosen', () => {
    const result = toInput(form({ size: CUSTOM_SIZE, customSize: ' 1600 × 900 ' }), models);
    expect(result.ok && result.input.size).toBe('1600x900');
    expect(toInput(form({ size: CUSTOM_SIZE, customSize: '1600' }), models)).toMatchObject({ ok: false });
  });

  it('keeps topic and resume only together', () => {
    const resumed = toInput(form({ topic: 'cat-20261018-140509', resume: true }), models);
    expect(resumed.ok && [resumed.input.topic, resumed.input.resume]).toEqual(['cat-20261018-140509', true]);
    const plain = toInput(form({ topic: null, resume: true }), models);
    expect(plain.ok && [plain.input.topic, plain.input.resume]).toEqual([null, false]);
  });

  it('lists every reason the run cannot start', () => {
    const result = toInput(form({ connection: null, prompt: '  \n ', model: 'nope', count: 0 }), models);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toEqual(['연결을 고르세요.', '프롬프트를 입력하세요.', '모델을 고르세요.', `장수는 1장부터 ${MAX_COUNT}장까지입니다.`]);
  });

  it('refuses transparency and references the model cannot do', () => {
    const transparent = toInput(form({ transparent: true }), models);
    expect(transparent).toEqual({ ok: false, errors: ['이 모델은 투명 배경을 지원하지 않습니다.'] });
    const noRefs = toInput(form({ model: 'grok-imagine-image-2.0', references: [ref('a.png')] }), models);
    expect(noRefs).toEqual({ ok: false, errors: ['이 모델은 참조 이미지를 쓸 수 없습니다.'] });
    const tooMany = toInput(form({ model: 'grok-imagine-image', references: [ref('a.png'), ref('b.png')] }), models);
    expect(tooMany).toEqual({ ok: false, errors: ['이 모델은 참조 이미지를 1장까지 받습니다.'] });
  });

  it('enforces the count limit 1..=50 and whole numbers', () => {
    expect(toInput(form({ count: 50 }), models).ok).toBe(true);
    expect(toInput(form({ count: 51 }), models).ok).toBe(false);
    expect(toInput(form({ count: 2.5 }), models).ok).toBe(false);
  });
});

describe('capabilities', () => {
  it('allows transparency only on models that make it themselves', () => {
    expect(capabilities(models[1])).toMatchObject({ transparency: true, transparencyReason: '' });
    expect(capabilities(models[0])).toMatchObject({ transparency: false, transparencyReason: '이 모델은 투명 배경을 지원하지 않습니다.' });
  });

  it('gives the reference limit and why references are off', () => {
    expect(capabilities(models[0])).toMatchObject({ references: true, maxReferences: 4, referencesReason: '' });
    expect(capabilities(models[2])).toMatchObject({ references: true, maxReferences: 1 });
    expect(capabilities(models[3])).toMatchObject({ references: false, maxReferences: 0, referencesReason: '이 모델은 참조 이미지를 쓸 수 없습니다.' });
  });

  it('turns everything off without a model', () => {
    expect(capabilities(undefined)).toEqual({
      references: false,
      maxReferences: 0,
      referencesReason: '모델을 먼저 고르세요.',
      transparency: false,
      transparencyReason: '모델을 먼저 고르세요.',
    });
  });
});

describe('addReferences', () => {
  it('adds up to the limit and says what was left out', () => {
    const current = [ref('a.png'), ref('b.png'), ref('c.png')];
    const result = addReferences(current, [ref('d.png'), ref('e.png')], 4);
    expect(result.references.map((r) => r.name)).toEqual(['a.png', 'b.png', 'c.png', 'd.png']);
    expect(result.rejected).toEqual(['참조 이미지는 최대 4장입니다: e.png']);
  });

  it('skips a file already added (paths compared without case) and files over 10MB', () => {
    const result = addReferences([ref('a.png')], [{ ...ref('A.PNG'), path: 'd:/REFS/a.png' }, ref('big.png', 10 * MB + 1)], 4);
    expect(result.references.map((r) => r.name)).toEqual(['a.png']);
    expect(result.rejected).toEqual(['이미 추가한 이미지입니다: A.PNG', '참조 이미지는 장당 10MB 이하여야 합니다: big.png']);
  });

  it('refuses everything when the model takes no references', () => {
    expect(addReferences([], [ref('a.png')], 0).rejected).toEqual(['이 모델은 참조 이미지를 쓸 수 없습니다: a.png']);
  });
});

describe('parseSize', () => {
  it('accepts W x H with x, X or × and spaces', () => {
    expect(parseSize('1920x1080')).toEqual({ ok: true, size: '1920x1080' });
    expect(parseSize(' 1080 × 1920 ')).toEqual({ ok: true, size: '1080x1920' });
    expect(parseSize('512X512')).toEqual({ ok: true, size: '512x512' });
  });

  it('refuses other text and sides outside 16..=8192', () => {
    for (const bad of ['', '1920', 'axb', '1920x', '0x100', '-5x100', '1.5x100', '8193x100', '15x100', '100x100x3']) {
      expect(parseSize(bad).ok, bad).toBe(false);
    }
    expect(parseSize('8192x16').ok).toBe(true);
    expect(parseSize('99999x1')).toEqual({ ok: false, message: '가로와 세로는 16~8192 사이여야 합니다.' });
    expect(parseSize('big')).toEqual({ ok: false, message: '"가로x세로" 형식으로 입력하세요(예: 1600x900).' });
  });

  it('sizeOf reads the preset or the custom field', () => {
    expect(sizeOf(form({ size: '1024x1536' }))).toBe('1024x1536');
    expect(sizeOf(form({ size: CUSTOM_SIZE, customSize: '640 x 480' }))).toBe('640x480');
    expect(sizeOf(form({ size: CUSTOM_SIZE, customSize: 'nope' }))).toBe(null);
  });
});

describe('connection state', () => {
  const none: ConnectionsView = { profiles: [], env: null, active: null };
  const two: ConnectionsView = { profiles: [profile('local', true), profile('회사')], env: null, active: 'local' };
  const env: ConnectionsView = { profiles: [profile('local', true)], env: { baseUrl: 'http://100.1.2.3:8317/v1', source: 'user_env', keyConfigured: true }, active: 'env' };

  it('tells loading, error, no connection and ready apart', () => {
    expect(connectionState(null, null)).toBe('loading');
    expect(connectionState(null, { category: 'invalid_request', message: 'x' })).toBe('error');
    expect(connectionState(none, null)).toBe('none');
    expect(connectionState(two, null)).toBe('ready');
  });

  it('defaults to the active connection, else the first profile', () => {
    expect(defaultConnection(two)).toBe('local');
    expect(defaultConnection(env)).toBe('env');
    expect(defaultConnection({ ...two, active: null })).toBe('local');
    expect(defaultConnection(none)).toBe(null);
    expect(defaultConnection(null)).toBe(null);
  });

  it('lists the environment connection first, then the profiles', () => {
    expect(connectionChoices(env).map((c) => [c.value, c.label])).toEqual([
      ['env', '환경변수 연결'],
      ['local', 'local · CLIProxyAPI (기본)'],
    ]);
    expect(connectionChoices(two).map((c) => c.value)).toEqual(['local', '회사']);
  });

  it('defaultModel keeps the current or last model when listed, else the first', () => {
    expect(defaultModel(models, 'gpt-image-2.5-sunburst', null)).toBe('gpt-image-2.5-sunburst');
    expect(defaultModel(models, 'gone', 'grok-imagine-image')).toBe('grok-imagine-image');
    expect(defaultModel(models, '', 'gone')).toBe('gpt-image-2.5');
    expect(defaultModel([], 'x', 'y')).toBe('');
  });
});

describe('reusing a record', () => {
  it('splits the stored prompt back into prompt and Avoid', () => {
    expect(splitPrompt('cat\nAvoid: text, logo')).toEqual({ prompt: 'cat', avoid: 'text, logo' });
    expect(splitPrompt('첫 줄\n둘째 줄')).toEqual({ prompt: '첫 줄\n둘째 줄', avoid: '' });
    expect(splitPrompt('a\nAvoid: x\nAvoid: y')).toEqual({ prompt: 'a\nAvoid: x', avoid: 'y' });
  });

  it('fills the form from a record (custom size when it is not a preset)', () => {
    const filled = formFromItem(
      item({ prompt: 'mug\nAvoid: text', imageModel: 'gpt-image-2.5-sunburst', size: '1600x900', quality: 'high', background: 'transparent', references: ['D:\\r.png'] }),
    );
    expect(filled).toEqual({
      prompt: 'mug',
      avoid: 'text',
      model: 'gpt-image-2.5-sunburst',
      size: CUSTOM_SIZE,
      customSize: '1600x900',
      quality: 'high',
      transparent: true,
      referencePaths: ['D:\\r.png'],
      topic: null,
      resume: false,
    });
    expect(formFromItem(item({ size: '1920x1080', quality: null })).size).toBe('1920x1080');
    expect(formFromItem(item({ quality: null })).quality).toBe('high');
  });

  it('resume keeps the topic and covers the highest image number of the group', () => {
    const group = [
      item({ imagePath: 'C:\\w\\ImageGallery\\output\\t\\t_1.png' }),
      item({ imagePath: 'C:\\w\\ImageGallery\\output\\t\\t_4.png', status: 'failed' }),
    ];
    expect(resumeFrom(group)).toMatchObject({ topic: 't', resume: true, count: 4, prompt: '고양이' });
    expect(imageNumber('C:\\x\\t_12.png')).toBe(12);
    expect(imageNumber('C:\\x\\t.png')).toBe(null);
  });

  it('resume is offered only for records of the work folder gallery', () => {
    const folders: FolderView[] = [
      { path: 'C:\\w', galleryRoot: 'C:\\w\\ImageGallery', available: true, isWorkDir: true },
      { path: 'E:\\old', galleryRoot: 'E:\\old\\ImageGallery', available: true, isWorkDir: false },
    ];
    expect(resumeBlocked('C:\\w\\ImageGallery', folders)).toBe(null);
    expect(resumeBlocked('c:/W/ImageGallery', folders)).toBe(null);
    expect(resumeBlocked('E:\\old\\ImageGallery', folders)).toBe(RESUME_WORK_DIR_ONLY);
    // Unknown folder (gallery not loaded yet, or the work folder changed): never resume blindly.
    expect(resumeBlocked('D:\\x\\ImageGallery', folders)).toBe(RESUME_WORK_DIR_ONLY);
    expect(resumeBlocked('C:\\w\\ImageGallery', [])).toBe(RESUME_WORK_DIR_ONLY);
  });

  it('samePath ignores case and slash direction', () => {
    expect(samePath('C:\\W\\a.png', 'c:/w/A.png')).toBe(true);
    expect(samePath('C:\\W\\a.png', 'c:/w/b.png')).toBe(false);
  });
});

describe('history', () => {
  it('groups records by folder and topic, newest group first, images by number', () => {
    const items = [
      item({ topic: 'b', imagePath: 'C:\\w\\o\\b\\b_2.png', modified: 50 }),
      item({ topic: 'a', imagePath: 'C:\\w\\o\\a\\a_1.png', modified: 300 }),
      item({ topic: 'b', imagePath: 'C:\\w\\o\\b\\b_1.png', modified: 60, status: 'failed' }),
      item({ topic: 'b', folder: 'D:\\p\\ImageGallery', imagePath: 'D:\\p\\o\\b\\b_1.png', modified: 10 }),
    ];
    const groups = historyGroups(items);
    expect(groups.map((g) => [g.topic, g.folder, g.items.length])).toEqual([
      ['a', 'C:\\w\\ImageGallery', 1],
      ['b', 'C:\\w\\ImageGallery', 2],
      ['b', 'D:\\p\\ImageGallery', 1],
    ]);
    const b = groups[1];
    expect(b.items.map((i) => imageNumber(i.imagePath))).toEqual([1, 2]);
    expect(b.modified).toBe(60);
    expect(b.counts).toEqual({ done: 1, failed: 1, other: 0 });
    expect(b.thumbs).toEqual(['C:\\w\\o\\b\\b_2.png']);
    expect(historyGroups(items, 1)).toHaveLength(1);
  });
});

describe('cards', () => {
  it('shows a run card with its image only when the image is there', () => {
    expect(runCard({ index: 2, status: 'running', output: 'C:\\o\\t_2.png' })).toMatchObject({ key: 'run-2', label: '#2', status: 'running', imagePath: null });
    expect(runCard({ index: 2, status: 'done', output: 'C:\\o\\t_2.png' })).toMatchObject({ status: 'done', imagePath: 'C:\\o\\t_2.png' });
    expect(runCard({ index: 2, status: 'done', output: 'C:\\o\\t_2.png' }).detail).toBe('');
  });

  it('puts the Korean cause first and keeps the raw provider message as the detail', () => {
    const reason = 'HTTP 429: You exceeded your current quota (billing_hard_limit_reached)';
    expect(runCard({ index: 3, status: 'failed', reason, category: 'quota' })).toMatchObject({ status: 'failed', text: '사용 한도 초과', detail: reason });
    // imageFailed carries no category; the finished event adds it later.
    expect(runCard({ index: 3, status: 'failed', reason: 'boom' })).toMatchObject({ status: 'failed', text: '', detail: 'boom' });
    expect(runCard({ index: 3, status: 'retrying', attempt: 2, reason: 'timeout' })).toMatchObject({ text: '재시도 2회차 · timeout', detail: '' });
  });

  it('shows a cancelled image as cancelled, not as a red failure', () => {
    const card = runCard({ index: 4, status: 'failed', reason: '취소되었습니다', category: 'cancelled' });
    expect(card).toMatchObject({ status: 'cancelled', text: '', detail: '' });
    const cards = [card, runCard({ index: 5, status: 'failed', category: 'quota' }), runCard({ index: 6, status: 'done', output: 'x' })];
    expect(tallyCards(cards)).toEqual({ done: 1, running: 0, retrying: 0, failed: 1, cancelled: 1, total: 3 });
  });

  it('while cancelling, an image that stops (imageFailed without a category) is cancelled, never counted as failed', () => {
    const stopped = { index: 1, status: 'failed' as const, reason: 'cancelled' };
    expect(runCard(stopped, false, true)).toMatchObject({ status: 'cancelled', text: '', detail: '' });
    expect(tallyCards([runCard(stopped, false, true)])).toMatchObject({ failed: 0, cancelled: 1 });
    // A real failure with its category stays a failure even while cancelling.
    expect(runCard({ ...stopped, category: 'quota' }, false, true).status).toBe('failed');
  });

  it('maps gallery statuses to card states', () => {
    expect(itemCard(item({ status: 'legacy' }))).toMatchObject({ status: 'done', imagePath: item().imagePath, label: '#1' });
    expect(itemCard(item({ status: 'missing' }))).toMatchObject({ status: 'missing', imagePath: null, text: '기록은 있지만 파일 없음' });
    expect(itemCard(item({ status: 'failed', error: 'HTTP 500' }))).toMatchObject({ status: 'failed', text: '', detail: 'HTTP 500' });
    expect(itemCard(item({ status: 'pending' }))).toMatchObject({ status: 'pending', text: '끝나지 않은 기록' });
    expect(itemCard(item({ background: 'transparent' })).transparent).toBe(true);
  });
});

describe('run header', () => {
  const tally = (over: Partial<ReturnType<typeof tallyCards>> = {}) => ({ done: 0, running: 0, retrying: 0, failed: 0, cancelled: 0, total: 6, ...over });

  it('shows only the live counters that are not zero', () => {
    expect(runPills(tally({ done: 2, running: 3 })).map((p) => p.label)).toEqual(['완료 2', '진행 3']);
    expect(runPills(tally({ retrying: 1, failed: 1, cancelled: 2 })).map((p) => p.tone)).toEqual(['warn', 'fail', 'neutral']);
    expect(runPills(tally())).toEqual([]);
  });

  it('sums up a finished run in one line', () => {
    expect(runOutcome(tally({ done: 5, failed: 1 }), 3.6, false)).toEqual({ line: '5장 완료 · 1장 실패 · 3.6초', tone: 'warn' });
    expect(runOutcome(tally({ done: 6 }), 12, false)).toEqual({ line: '6장 완료 · 12초', tone: 'ok' });
    expect(runOutcome(tally({ done: 0, cancelled: 6 }), 2.1, true)).toEqual({ line: '취소됨 · 완료 0/6 · 2.1초', tone: 'neutral' });
    expect(runOutcome(tally({ done: 2, failed: 1, cancelled: 3 }), 5, true).line).toBe('취소됨 · 완료 2/6 · 1장 실패 · 5초');
  });

  it('the bar of a cancelled run shows only what was made, in a neutral colour', () => {
    expect(progressView(tally({ cancelled: 6 }), false, true)).toEqual({ fraction: 0, tone: 'neutral' });
    expect(progressView(tally({ done: 3, cancelled: 3 }), false, true)).toEqual({ fraction: 0.5, tone: 'neutral' });
    expect(progressView(tally({ done: 2, running: 1, cancelled: 1 }), true, false)).toEqual({ fraction: 0.5, tone: 'accent' });
    expect(progressView(tally({ done: 5, failed: 1 }), false, false)).toEqual({ fraction: 1, tone: 'warn' });
    expect(progressView(tally({ total: 0 }), false, false)).toEqual({ fraction: 0, tone: 'accent' });
  });

  it('retrying failed images resumes the same topic with the same settings', () => {
    const input = {
      profile: 'local',
      prompt: 'p',
      avoid: '',
      imageModel: 'gpt-image-2.5',
      size: '1024x1024',
      quality: 'low',
      count: 6,
      references: ['C:\\r.png'],
      background: 'auto' as const,
      topic: null,
      resume: false,
    };
    expect(retryInput(input, 'cozy-room')).toEqual({ ...input, topic: 'cozy-room', resume: true });
    expect(input.resume).toBe(false);
  });
});
