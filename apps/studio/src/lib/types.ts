// TypeScript shapes of what the Rust side (apps/studio/src-tauri) serializes.
// Service types are camelCase; types reused from image-core (ConnectionReport, GenerationStatus, EngineError) stay snake_case
// (stage 3 ledger, Task 2 ruling).

/** image_core::error::Category (snake_case). */
export type Category =
  | 'network'
  | 'dns'
  | 'timeout'
  | 'tls'
  | 'auth'
  | 'permission'
  | 'quota'
  | 'rate_limit'
  | 'conflict'
  | 'cancelled'
  | 'not_found'
  | 'redirect'
  | 'unsupported'
  | 'invalid_request'
  | 'provider'
  | 'internal';

/** image_core::profiles::ProviderKind (snake_case). */
export type ProviderKind = 'cliproxy' | 'openai' | 'gemini' | 'xai' | 'openai_compatible';

/** Error of every command (service::ServiceError). Never carries a key. */
export interface ServiceError {
  category: Category;
  message: string;
}

/** image_core::error::EngineError (snake_case). */
export interface EngineError {
  category: Category;
  provider_code: string | null;
  http_status: number | null;
  retryable: boolean;
  message: string;
}

/** image_core::profiles::GenerationStatus (snake_case): last real generation through a profile. */
export interface GenerationStatus {
  ok: boolean;
  /** Seconds since the Unix epoch. */
  at: number;
  model: string;
  category: Category | null;
  provider_code: string | null;
}

/** image_core::check::ConnectionReport (snake_case). */
export interface ConnectionReport {
  profile: string | null;
  kind: ProviderKind;
  base_url: string;
  key_configured: boolean;
  reachable: boolean;
  authenticated: boolean;
  models_listed: boolean;
  image_models: string[];
  last_generation: GenerationStatus | null;
  warning: string | null;
  error: EngineError | null;
}

export interface AppInfo {
  version: string;
  /** Folder of profiles.json and settings.json. */
  home: string;
  workDir: string;
}

export interface Settings {
  workDir: string;
  /** Extra gallery folders; the work folder is always shown and is not listed here. */
  galleryFolders: string[];
  lastProfile: string | null;
  lastModel: string | null;
  /** Chat model of "영어로 다듬기"; null = suggest one. Older settings files lack it. */
  enhanceModel?: string | null;
  /** Galleries found on this PC that the user took out of the list. */
  hiddenFolders?: string[];
  /** "이 PC 공유". */
  share?: ShareSettings;
  /** Other PCs whose shared galleries the gallery shows (their tokens stay in the key store). */
  peers?: Peer[];
}

export interface ShareSettings {
  enabled: boolean;
  port: number;
}

export interface Peer {
  id: string;
  name: string;
  /** http://<address>:<port> of that PC's "이 PC 공유". */
  url: string;
}

/** "이 PC 공유" as the screen shows it. */
export interface ShareView {
  enabled: boolean;
  port: number;
  running: boolean;
  /** This PC's Tailscale address, null when Tailscale is not connected. */
  address: string | null;
  name: string;
  /** What another PC needs to read this PC's galleries. */
  token: string;
  error: string | null;
}

export interface ShareInfo {
  name: string;
  version: string;
}

export interface ProfileView {
  id: string;
  name: string;
  kind: ProviderKind;
  kindLabel: string;
  baseUrl: string;
  concurrency: number | null;
  isDefault: boolean;
  keyConfigured: boolean;
  /** Plain-http warning for a public address. */
  warning: string | null;
  lastGeneration: GenerationStatus | null;
}

export interface EnvConnectionView {
  baseUrl: string;
  source: string;
  keyConfigured: boolean;
}

export interface ConnectionsView {
  profiles: ProfileView[];
  env: EnvConnectionView | null;
  /** Name of the connection used now (resolution order), "env", or null. */
  active: string | null;
}

export interface ProfileInput {
  /** null = new profile. */
  id: string | null;
  name: string;
  kind: ProviderKind;
  /** null or blank = the kind's official address. */
  baseUrl: string | null;
  concurrency: number | null;
  /** null = keep the stored key, "" = delete it, other = replace. */
  key: string | null;
  makeDefault: boolean;
}

export interface ModelView {
  id: string;
  provider: string;
  references: boolean;
  /** References the screen may attach for this model (at most 4; 0 = none). */
  maxReferences: number;
  transparency: boolean;
}

/** A checked reference image (inspect_reference). */
export interface ReferenceView {
  path: string;
  name: string;
  bytes: number;
  width: number;
  height: number;
}

export type Background = 'auto' | 'opaque' | 'transparent';

/** How a provider image of another shape is saved: whole with transparent margins, untouched at its own size, or cropped. */
export type Fit = 'contain' | 'original' | 'cover';

/** service::enhance::EnhanceInput. */
export interface EnhanceInput {
  /** Profile name; null = the active connection. */
  profile: string | null;
  prompt: string;
  avoid: string;
  imageModel: string | null;
  /** Chat model that writes the prompt. */
  model: string;
}

/** service::enhance::EnhanceView. */
export interface EnhanceView {
  prompt: string;
  /** Model the provider reported, else the requested one. */
  model: string;
}

export interface GenerateInput {
  profile: string | null;
  prompt: string;
  avoid: string;
  /** The user's own request when `prompt` came from "영어로 다듬기"; recorded with each image, never sent. */
  originalPrompt?: string | null;
  imageModel: string;
  size: string;
  quality: string;
  count: number;
  references: string[];
  background: Background;
  fit: Fit;
  topic: string | null;
  resume: boolean;
  /** Mask edit of references[0]: everything outside the painted area stays as it is. */
  edit?: EditInput | null;
}

/** Painted layers of the editor, PNG data URLs of the source size. */
export interface EditInput {
  /** Area to change (painted pixels have alpha above 0). */
  mask: string;
  /** Drawing over the image, sent as part of the picture; drawn pixels count as marked. */
  strokes: string | null;
}

/** Progress of "배경 제거 (내 PC)". */
export type MatteEvent = { event: 'downloading'; data: { done: number; total: number } } | { event: 'removing' };

export interface MatteResult {
  output: string;
  topic: string;
  transparentRatio: number;
}

export interface FailureView {
  index: number;
  reason: string;
  category: Category | null;
  providerCode: string | null;
}

export interface RunSummary {
  ok: boolean;
  outputs: string[];
  failures: FailureView[];
  topic: string;
  outputDir: string;
  elapsedSec: number;
  cancelled: boolean;
  warnings: string[];
}

/** Progress of one run (service::generate::RunEvent); image indexes start at 1. */
export type RunEvent =
  | { event: 'started'; data: { runId: string; total: number; outputDir: string; topic: string } }
  | { event: 'imageStarted'; data: { index: number; output: string } }
  | { event: 'imageRetry'; data: { index: number; attempt: number; delaySec: number; reason: string } }
  | { event: 'imageFinished'; data: { index: number; output: string; elapsedSec: number } }
  | { event: 'imageSkipped'; data: { index: number; output: string } }
  | { event: 'imageFailed'; data: { index: number; reason: string } }
  | { event: 'finished'; data: { summary: RunSummary } };

export type GalleryStatus = 'complete' | 'pending' | 'failed' | 'legacy' | 'missing';

export interface GalleryItem {
  imagePath: string;
  recordPath: string;
  /** Gallery root the record came from. */
  folder: string;
  topic: string;
  status: GalleryStatus;
  prompt: string;
  /** The user's request before "영어로 다듬기" rewrote `prompt`. */
  originalPrompt?: string | null;
  imageModel: string;
  responseModel: string | null;
  size: string;
  background: string | null;
  transparentRatio: number | null;
  error: string | null;
  /** Record file mtime, seconds since the Unix epoch. */
  modified: number;
  quality: string | null;
  /** reference_images of the record. */
  references: string[];
  /** Size the provider sent, before fitting to savedSize. */
  providerSize: string | null;
  savedSize: string | null;
  requestId: string | null;
  /** Requested fit; older records may hold "none" (what was applied) and Python records nothing. */
  fit: string | null;
  /** generate | edit | auto | remove_background (local cut-out). */
  action?: string | null;
  /** Mask file of a partial edit. */
  mask?: string | null;
  /** Other PC it came from; null = this PC. Its paths are then peer://<id>/<path there>. */
  machine?: string | null;
}

/** work = the work folder, added = "폴더 추가", found = found on this PC (CLI runs, "이 PC에서 찾기"), peer = another PC. */
export type FolderSource = 'work' | 'added' | 'found' | 'peer';

export interface FolderView {
  /** Path as registered. */
  path: string;
  galleryRoot: string;
  available: boolean;
  isWorkDir: boolean;
  source?: FolderSource;
  /** Other PC's name (source peer). */
  machine?: string | null;
  /** Why another PC could not be read. */
  note?: string | null;
}

export interface GalleryView {
  items: GalleryItem[];
  folders: FolderView[];
  truncated: boolean;
  /** "이 PC에서 찾기" has run at least once. */
  scanned?: boolean;
}

export type Screen = 'generate' | 'gallery' | 'connections';
