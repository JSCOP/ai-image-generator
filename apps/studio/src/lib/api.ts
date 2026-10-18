// Typed wrappers of the Tauri commands (apps/studio/src-tauri/src/commands.rs). Command names and argument keys live here only.
// Every function rejects with a ServiceError; no command returns a key value.

import { Channel, convertFileSrc, invoke } from '@tauri-apps/api/core';
import { categoryLabel } from './format';
import type {
  AppInfo,
  ConnectionReport,
  ConnectionsView,
  GalleryView,
  GenerateInput,
  ModelView,
  ProfileInput,
  ProfileView,
  RunEvent,
  RunSummary,
  ServiceError,
  Settings,
} from './types';

export function isServiceError(value: unknown): value is ServiceError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as ServiceError).category === 'string' &&
    typeof (value as ServiceError).message === 'string'
  );
}

/** Any rejection as a ServiceError (outside the app window, invoke itself throws a TypeError). */
export function toServiceError(value: unknown): ServiceError {
  if (isServiceError(value)) return { category: value.category, message: value.message };
  const text = value instanceof Error ? value.message : String(value);
  return { category: 'internal', message: `앱과 연결되지 않았습니다: ${text}` };
}

/** "키가 거부됨 · 401 Unauthorized". */
export function errorText(error: ServiceError): string {
  return error.message ? `${categoryLabel(error.category)} · ${error.message}` : categoryLabel(error.category);
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toServiceError(e);
  }
}

export const appInfo = () => call<AppInfo>('app_info');
export const getSettings = () => call<Settings>('get_settings');
export const saveSettings = (settings: Settings) => call<void>('save_settings', { settings });

export const connections = () => call<ConnectionsView>('connections');
export const saveProfile = (input: ProfileInput) => call<ProfileView>('save_profile', { input });
export const removeProfile = (id: string) => call<void>('remove_profile', { id });
/** null clears the default. */
export const setDefaultProfile = (id: string | null) => call<void>('set_default_profile', { id });
/** name null = the active connection. No paid request is made. */
export const checkProfile = (name: string | null) => call<ConnectionReport>('check_profile', { name });
export const listModels = (name: string | null) => call<ModelView[]>('list_models', { name });

/**
 * Runs one generation. Progress arrives on `onEvent` (first `started` carries the run id for cancelGeneration);
 * resolves with the summary. A rejection before `started` means nothing was sent to the provider.
 */
export function startGeneration(input: GenerateInput, onEvent: (event: RunEvent) => void): Promise<RunSummary> {
  const channel = new Channel<RunEvent>();
  channel.onmessage = onEvent;
  return call<RunSummary>('start_generation', { input, onEvent: channel });
}

/** false when no run has this id (already ended). */
export const cancelGeneration = (runId: string) => call<boolean>('cancel_generation', { runId });

export const gallery = (limit?: number) => call<GalleryView>('gallery', { limit: limit ?? null });
/** Returns the saved settings. */
export const addGalleryFolder = (path: string) => call<Settings>('add_gallery_folder', { path });
export const removeGalleryFolder = (path: string) => call<Settings>('remove_gallery_folder', { path });

/** URL of a local image for <img src> (asset protocol; the folder must be a registered gallery). */
export function imageSrc(path: string): string {
  return convertFileSrc(path);
}
