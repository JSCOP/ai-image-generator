import { describe, expect, it } from 'vitest';
import { cardText, counts, failRun, reduce, type Card } from './run';
import type { RunEvent, RunSummary } from './types';

function summary(over: Partial<RunSummary> = {}): RunSummary {
  return {
    ok: true,
    outputs: [],
    failures: [],
    topic: 't',
    outputDir: 'C:/w/ImageGallery/output/t',
    elapsedSec: 1,
    cancelled: false,
    warnings: [],
    ...over,
  };
}

function play(events: RunEvent[], start: Card[] = []): Card[] {
  return events.reduce(reduce, start);
}

const started: RunEvent = { event: 'started', data: { runId: 'run-1', total: 3, outputDir: 'C:/w/out/t', topic: 't' } };

describe('reduce', () => {
  it('started makes one waiting card per image, numbered from 1', () => {
    const cards = play([started]);
    expect(cards.map((c) => [c.index, c.status])).toEqual([
      [1, 'waiting'],
      [2, 'waiting'],
      [3, 'waiting'],
    ]);
    expect(counts(cards)).toEqual({ done: 0, running: 0, retrying: 0, failed: 0, total: 3 });
  });

  it('follows started -> retry -> running -> done for one card', () => {
    let cards = play([started, { event: 'imageStarted', data: { index: 2, output: 'C:/o/t_2.png' } }]);
    expect(cards[1]).toMatchObject({ status: 'running', output: 'C:/o/t_2.png' });
    expect(counts(cards).running).toBe(1);

    cards = reduce(cards, { event: 'imageRetry', data: { index: 2, attempt: 1, delaySec: 5, reason: 'HTTP 429 too many requests' } });
    expect(cards[1]).toMatchObject({ status: 'retrying', attempt: 1, reason: 'HTTP 429 too many requests', delaySec: 5 });
    expect(counts(cards)).toMatchObject({ running: 0, retrying: 1 });

    // The engine sends no new "started" after a retry wait; the finish ends it.
    cards = reduce(cards, { event: 'imageFinished', data: { index: 2, output: 'C:/o/t_2.png', elapsedSec: 12.5 } });
    expect(cards[1]).toMatchObject({ status: 'done', output: 'C:/o/t_2.png', elapsedSec: 12.5, reason: undefined });
    expect(counts(cards)).toEqual({ done: 1, running: 0, retrying: 0, failed: 0, total: 3 });
  });

  it('counts a skipped (already made) image as done', () => {
    const cards = play([started, { event: 'imageSkipped', data: { index: 1, output: 'C:/o/t_1.png' } }]);
    expect(cards[0]).toMatchObject({ status: 'skipped', output: 'C:/o/t_1.png' });
    expect(counts(cards).done).toBe(1);
  });

  it('marks a failed image and later gives it the category from the summary', () => {
    let cards = play([
      started,
      { event: 'imageStarted', data: { index: 3, output: 'C:/o/t_3.png' } },
      { event: 'imageFailed', data: { index: 3, reason: 'billing hard limit reached' } },
    ]);
    expect(cards[2]).toMatchObject({ status: 'failed', reason: 'billing hard limit reached' });
    expect(counts(cards).failed).toBe(1);

    cards = reduce(cards, {
      event: 'finished',
      data: {
        summary: summary({
          ok: false,
          failures: [{ index: 3, reason: 'billing hard limit reached', category: 'quota', providerCode: 'billing_hard_limit_reached' }],
        }),
      },
    });
    expect(cards[2]).toMatchObject({ status: 'failed', category: 'quota', providerCode: 'billing_hard_limit_reached' });
    expect(cardText(cards[2])).toBe('사용 한도 초과 · billing hard limit reached');
  });

  it('a cancelled run fails the cards that never finished, keeping finished ones', () => {
    const cards = play([
      started,
      { event: 'imageFinished', data: { index: 1, output: 'C:/o/t_1.png', elapsedSec: 3 } },
      { event: 'imageStarted', data: { index: 2, output: 'C:/o/t_2.png' } },
      {
        event: 'finished',
        data: {
          summary: summary({
            ok: false,
            cancelled: true,
            outputs: ['C:/o/t_1.png'],
            // Not started before the cancel: no image event, only this failure (ledger Task 3 ruling).
            failures: [{ index: 3, reason: '취소되었습니다', category: 'cancelled', providerCode: null }],
          }),
        },
      },
    ]);
    expect(cards.map((c) => c.status)).toEqual(['done', 'failed', 'failed']);
    expect(cards[1].category).toBe('cancelled');
    expect(cards[2]).toMatchObject({ category: 'cancelled', reason: '취소되었습니다' });
    expect(cardText(cards[1])).toBe('취소됨');
    expect(counts(cards)).toEqual({ done: 1, running: 0, retrying: 0, failed: 2, total: 3 });
  });

  it('an event for an unknown index adds a card instead of being lost', () => {
    const cards = play([{ event: 'imageStarted', data: { index: 4, output: 'C:/o/t_4.png' } }]);
    expect(cards).toHaveLength(1);
    expect(cards[0]).toMatchObject({ index: 4, status: 'running' });
  });

  it('does not change the input array (pure)', () => {
    const before = play([started]);
    const copy = structuredClone(before);
    reduce(before, { event: 'imageFinished', data: { index: 1, output: 'x', elapsedSec: 1 } });
    expect(before).toEqual(copy);
  });

  it('a second started (resume run) starts a fresh set of cards', () => {
    const first = play([started, { event: 'imageFinished', data: { index: 1, output: 'x', elapsedSec: 1 } }]);
    const again = reduce(first, { event: 'started', data: { runId: 'run-2', total: 2, outputDir: 'o', topic: 't' } });
    expect(again.map((c) => c.status)).toEqual(['waiting', 'waiting']);
  });
});

describe('failRun', () => {
  it('fails unfinished cards with the error category when the command is rejected', () => {
    const cards = play([started, { event: 'imageFinished', data: { index: 1, output: 'x', elapsedSec: 1 } }]);
    const out = failRun(cards, { category: 'internal', message: '출력 폴더를 만들 수 없습니다' });
    expect(out.map((c) => c.status)).toEqual(['done', 'failed', 'failed']);
    expect(cardText(out[2])).toBe('앱 내부 오류 · 출력 폴더를 만들 수 없습니다');
  });
});

describe('cardText', () => {
  it('shows the retry attempt and reason', () => {
    const [card] = play([
      { event: 'started', data: { runId: 'r', total: 1, outputDir: 'o', topic: 't' } },
      { event: 'imageRetry', data: { index: 1, attempt: 2, delaySec: 10, reason: 'timeout after 120s' } },
    ]);
    expect(cardText(card)).toBe('재시도 2회차 · timeout after 120s');
  });

  it('is empty for a plain waiting card', () => {
    const [card] = play([{ event: 'started', data: { runId: 'r', total: 1, outputDir: 'o', topic: 't' } }]);
    expect(cardText(card)).toBe('');
  });
});
