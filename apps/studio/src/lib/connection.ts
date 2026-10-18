// Connection status helpers (pure): the four check stages and the active connection badge.

import { categoryLabel } from './format';
import type { ConnectionReport, ConnectionsView, GenerationStatus, ProfileView, Screen } from './types';

export type StageState = 'ok' | 'warn' | 'fail' | 'unknown';

export interface Stage {
  key: 'server' | 'auth' | 'models' | 'generation';
  label: string;
  state: StageState;
  /** Korean reason for warn/fail, else "". */
  detail: string;
}

function errorDetail(report: ConnectionReport): string {
  const e = report.error;
  if (!e) return '';
  return e.provider_code ? `${categoryLabel(e.category)} (${e.provider_code})` : categoryLabel(e.category);
}

/**
 * Server -> key -> model list -> real generation (spec §6.2). A stage after a failed one stays unknown.
 * `report` is the last check result (null = not checked); generation comes from the last real run only.
 */
export function stagesOf(report: ConnectionReport | null, lastGeneration: GenerationStatus | null): Stage[] {
  let server: StageState = 'unknown';
  let auth: StageState = 'unknown';
  let models: StageState = 'unknown';
  let serverDetail = '';
  let authDetail = '';
  let modelsDetail = '';
  if (report) {
    server = report.reachable ? 'ok' : 'fail';
    if (!report.reachable) serverDetail = errorDetail(report);
    else if (report.authenticated) {
      auth = 'ok';
      if (!report.models_listed) {
        models = 'fail';
        modelsDetail = errorDetail(report) || '모델 목록을 받지 못함';
      } else if (report.image_models.length === 0) {
        models = 'warn';
        modelsDetail = '이미지 모델이 없음';
      } else models = 'ok';
    } else {
      auth = 'fail';
      authDetail = errorDetail(report) || '키가 거부됨';
    }
  }
  const last = report?.last_generation ?? lastGeneration;
  let generation: StageState = 'unknown';
  let generationDetail = '';
  if (last) {
    generation = last.ok ? 'ok' : 'fail';
    if (!last.ok) {
      const label = last.category ? categoryLabel(last.category) : '실패';
      generationDetail = last.provider_code ? `${label} (${last.provider_code})` : label;
    }
  }
  return [
    { key: 'server', label: '서버', state: server, detail: serverDetail },
    { key: 'auth', label: '인증', state: auth, detail: authDetail },
    { key: 'models', label: '모델', state: models, detail: modelsDetail },
    { key: 'generation', label: '생성', state: generation, detail: generationDetail },
  ];
}

export interface ActiveConnection {
  /** Profile name, or "환경변수 연결". */
  name: string;
  kindLabel: string;
  /** The profile, or null for the environment connection. */
  profile: ProfileView | null;
}

export const ENV_NAME = '환경변수 연결';

/** The connection a generation would use now, or null when there is none. */
export function activeConnection(view: ConnectionsView | null): ActiveConnection | null {
  if (!view || !view.active) return null;
  if (view.active === 'env') return { name: ENV_NAME, kindLabel: 'CLIProxyAPI', profile: null };
  const profile = view.profiles.find((p) => p.name === view.active) ?? null;
  if (!profile) return null;
  return { name: profile.name, kindLabel: profile.kindLabel, profile };
}

/** First run without any connection opens the connections screen (spec §8.6). */
export function firstScreen(view: ConnectionsView | null): Screen {
  if (view && view.profiles.length === 0 && !view.env) return 'connections';
  return 'generate';
}
