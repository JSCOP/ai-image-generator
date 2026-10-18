// RunEvent -> result card state (pure; the generate screen keeps the array in $state).

import { categoryLabel } from './format';
import type { Category, RunEvent, ServiceError } from './types';

export type CardStatus = 'waiting' | 'running' | 'retrying' | 'done' | 'skipped' | 'failed';

export interface Card {
  /** Image number from 1, as the engine counts. */
  index: number;
  status: CardStatus;
  /** Output image path, known from imageStarted on. */
  output?: string;
  /** Retry or failure reason (engine text, key already removed). */
  reason?: string;
  category?: Category;
  providerCode?: string;
  attempt?: number;
  delaySec?: number;
  elapsedSec?: number;
}

export interface Counts {
  /** Finished, including skipped (already made, resume). */
  done: number;
  running: number;
  retrying: number;
  failed: number;
  total: number;
}

const FINISHED: ReadonlySet<CardStatus> = new Set(['done', 'skipped', 'failed']);

export function isFinished(card: Card): boolean {
  return FINISHED.has(card.status);
}

function update(cards: Card[], index: number, change: (card: Card) => Card): Card[] {
  const at = cards.findIndex((c) => c.index === index);
  if (at < 0) return [...cards, change({ index, status: 'waiting' })].sort((a, b) => a.index - b.index);
  return cards.map((c, i) => (i === at ? change(c) : c));
}

/** Next card list after one event; the input is not changed. */
export function reduce(cards: Card[], ev: RunEvent): Card[] {
  switch (ev.event) {
    case 'started':
      return Array.from({ length: ev.data.total }, (_, i) => ({ index: i + 1, status: 'waiting' }));
    case 'imageStarted':
      return update(cards, ev.data.index, (c) => ({ ...c, status: 'running', output: ev.data.output, reason: undefined }));
    case 'imageRetry':
      return update(cards, ev.data.index, (c) => ({
        ...c,
        status: 'retrying',
        attempt: ev.data.attempt,
        delaySec: ev.data.delaySec,
        reason: ev.data.reason,
      }));
    case 'imageFinished':
      return update(cards, ev.data.index, (c) => ({
        ...c,
        status: 'done',
        output: ev.data.output,
        elapsedSec: ev.data.elapsedSec,
        reason: undefined,
        category: undefined,
      }));
    case 'imageSkipped':
      return update(cards, ev.data.index, (c) => ({ ...c, status: 'skipped', output: ev.data.output, reason: undefined }));
    case 'imageFailed':
      return update(cards, ev.data.index, (c) => ({ ...c, status: 'failed', reason: ev.data.reason }));
    case 'finished': {
      const { summary } = ev.data;
      let next = cards;
      for (const f of summary.failures) {
        next = update(next, f.index, (c) => ({
          ...c,
          status: 'failed',
          reason: c.reason ?? f.reason,
          category: f.category ?? c.category,
          providerCode: f.providerCode ?? c.providerCode,
        }));
      }
      // Whatever never finished stopped with the run (cancel, or an engine stop).
      const category: Category = summary.cancelled ? 'cancelled' : 'internal';
      return next.map((c) => (isFinished(c) ? c : { ...c, status: 'failed', category: c.category ?? category }));
    }
  }
}

/** The start_generation promise was rejected after the run began: every unfinished card fails with that error. */
export function failRun(cards: Card[], error: ServiceError): Card[] {
  return cards.map((c) => (isFinished(c) ? c : { ...c, status: 'failed', category: error.category, reason: error.message }));
}

export function counts(cards: Card[]): Counts {
  const n: Counts = { done: 0, running: 0, retrying: 0, failed: 0, total: cards.length };
  for (const c of cards) {
    if (c.status === 'done' || c.status === 'skipped') n.done++;
    else if (c.status === 'running') n.running++;
    else if (c.status === 'retrying') n.retrying++;
    else if (c.status === 'failed') n.failed++;
  }
  return n;
}

/** Short Korean line under a card: retry attempt and reason, or failure category and reason. */
export function cardText(card: Card): string {
  if (card.status === 'retrying') {
    const head = card.attempt ? `재시도 ${card.attempt}회차` : '재시도';
    return card.reason ? `${head} · ${card.reason}` : head;
  }
  if (card.status === 'failed') {
    const head = card.category ? categoryLabel(card.category) : '실패';
    return card.reason ? `${head} · ${card.reason}` : head;
  }
  return '';
}
