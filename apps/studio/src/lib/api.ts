// Typed wrappers of the Tauri commands (apps/studio/src-tauri/src/commands.rs). Command names and argument keys live here only.
// Every function rejects with a ServiceError; no command returns a key value.

import { Channel, convertFileSrc, invoke } from '@tauri-apps/api/core';
import { categoryLabel } from './format';
import type {
  AppInfo,
  ConnectionReport,
  ConnectionsView,
  EnhanceInput,
  EnhanceView,
  GalleryView,
  GenerateInput,
  MatteEvent,
  MatteResult,
  ModelView,
  ProfileInput,
  ProfileView,
  ReferenceView,
  RunEvent,
  RunSummary,
  ServiceError,
  Settings,
  ShareInfo,
  ShareSettings,
  ShareView,
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
/** Chat models of a connection for "영어로 다듬기" (no paid request). */
export const textModels = (name: string | null) => call<string[]>('text_models', { name });
/** One chat call that rewrites the prompt in English; nothing is generated or saved. */
export const enhancePrompt = (input: EnhanceInput) => call<EnhanceView>('enhance_prompt', { input });

/**
 * Runs one generation. Progress arrives on `onEvent` (first `started` carries the run id for cancelGeneration);
 * resolves with the summary. A rejection before `started` means nothing was sent to the provider.
 */
export function startGeneration(input: GenerateInput, onEvent: (event: RunEvent) => void): Promise<RunSummary> {
  const channel = new Channel<RunEvent>();
  channel.onmessage = onEvent;
  return call<RunSummary>('start_generation', { input, onEvent: channel });
}

/** Checks a picked or dropped reference (file, <= 10MB, image, sides <= 8192) and allows its thumbnail. */
export const inspectReference = (path: string) => call<ReferenceView>('inspect_reference', { path });

/** Ctrl+V: saves a pasted image (data URL) into the work folder and returns it as a checked reference. */
export const pasteReference = (data: string) => call<ReferenceView>('paste_reference', { data });

/** "배경 제거 (내 PC)": a new transparent copy in the gallery; the first use downloads the model (progress on onEvent). */
export function removeBackground(path: string, onEvent: (event: MatteEvent) => void): Promise<MatteResult> {
  const channel = new Channel<MatteEvent>();
  channel.onmessage = onEvent;
  return call<MatteResult>('remove_background', { path, onEvent: channel });
}

/** false when no run has this id (already ended). */
export const cancelGeneration = (runId: string) => call<boolean>('cancel_generation', { runId });

/** peers false = this PC only (the generate screen's history). */
export const gallery = (limit?: number, peers = true) => call<GalleryView>('gallery', { limit: limit ?? null, peers });
/** Returns the saved settings. */
export const addGalleryFolder = (path: string) => call<Settings>('add_gallery_folder', { path });
export const removeGalleryFolder = (path: string) => call<Settings>('remove_gallery_folder', { path });
/** "이 PC에서 찾기": walks the fixed drives for ImageGallery folders; resolves with how many it found. */
export const discoverGalleries = () => call<number>('discover_galleries');

/** "이 PC 공유". */
export const shareStatus = () => call<ShareView>('share_status');
export const setShare = (share: ShareSettings) => call<ShareView>('set_share', { share });
export const newShareToken = () => call<ShareView>('new_share_token');
/** "다른 PC": checks without saving, saves after a check, forgets (token and cached images too). */
export const checkPeer = (url: string, token: string) => call<ShareInfo>('check_peer', { url, token });
export const addPeer = (name: string, url: string, token: string) => call<Settings>('add_peer', { name, url, token });
export const removePeer = (id: string) => call<Settings>('remove_peer', { id });

const PEER = 'peer://';

/** Whether a gallery path belongs to another PC (peer://<id>/<path there>). */
export function isRemotePath(path: string): boolean {
  return path.startsWith(PEER);
}

/**
 * URL of an image for <img src>: a local file through the asset protocol (the folder must be a listed gallery),
 * another PC's through the peer scheme (thumb = a small copy for cards and previews).
 */
export function imageSrc(path: string, thumb = false): string {
  if (isRemotePath(path)) return convertFileSrc(`${thumb ? 't' : 'f'}/${path.slice(PEER.length)}`, 'peer');
  return convertFileSrc(path);
}
