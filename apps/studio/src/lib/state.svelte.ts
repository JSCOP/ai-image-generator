// App-wide state (Svelte 5 runes). Screens read `app` and call these functions; pure logic lives in run.ts, connection.ts, format.ts.

import * as api from './api';
import { firstScreen } from './connection';
import type { AppInfo, ConnectionReport, ConnectionsView, Screen, ServiceError, Settings } from './types';

export const app = $state({
  screen: 'generate' as Screen,
  info: null as AppInfo | null,
  settings: null as Settings | null,
  connections: null as ConnectionsView | null,
  /** Last check result per connection: profile name, or "env". */
  checks: {} as Record<string, ConnectionReport>,
  loading: true,
  loadError: null as ServiceError | null,
});

export function navigate(screen: Screen): void {
  app.screen = screen;
}

/** First load: app info, settings and connections; opens the connections screen when there is none. */
export async function loadApp(): Promise<void> {
  app.loading = true;
  app.loadError = null;
  try {
    const [info, settings, connections] = await Promise.all([api.appInfo(), api.getSettings(), api.connections()]);
    app.info = info;
    app.settings = settings;
    app.connections = connections;
    app.screen = firstScreen(connections);
  } catch (e) {
    app.loadError = api.toServiceError(e);
  } finally {
    app.loading = false;
  }
}

export async function refreshConnections(): Promise<void> {
  try {
    app.connections = await api.connections();
  } catch (e) {
    toastError(api.toServiceError(e));
  }
}

/** Checks a connection (no paid request) and keeps the result for the status dots. */
export async function checkConnection(name: string | null): Promise<ConnectionReport | null> {
  try {
    const report = await api.checkProfile(name);
    app.checks[report.profile ?? 'env'] = report;
    return report;
  } catch (e) {
    toastError(api.toServiceError(e));
    return null;
  }
}

// ---- toasts ----

export type ToastTone = 'info' | 'ok' | 'warn' | 'error';

export interface Toast {
  id: number;
  tone: ToastTone;
  text: string;
}

export const toasts: Toast[] = $state([]);
let nextToast = 1;

export function toast(text: string, tone: ToastTone = 'info', ms = 4500): number {
  const id = nextToast++;
  toasts.push({ id, tone, text });
  if (ms > 0) setTimeout(() => dismissToast(id), ms);
  return id;
}

export function toastError(error: ServiceError): number {
  return toast(api.errorText(error), 'error', 8000);
}

export function dismissToast(id: number): void {
  const at = toasts.findIndex((t) => t.id === id);
  if (at >= 0) toasts.splice(at, 1);
}

// ---- in-app confirm dialog (no browser confirm()) ----

export interface AskOptions {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
}

export const confirmState = $state({
  open: false,
  options: { title: '', message: '' } as AskOptions,
});

let resolveAsk: ((ok: boolean) => void) | null = null;

/** Shows the app's confirm dialog; resolves true on confirm, false on cancel or Esc. */
export function ask(options: AskOptions): Promise<boolean> {
  resolveAsk?.(false);
  confirmState.options = options;
  confirmState.open = true;
  return new Promise((resolve) => {
    resolveAsk = resolve;
  });
}

export function answer(ok: boolean): void {
  confirmState.open = false;
  const resolve = resolveAsk;
  resolveAsk = null;
  resolve?.(ok);
}
