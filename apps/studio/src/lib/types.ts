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

export interface GenerateInput {
  profile: string | null;
  prompt: string;
  avoid: string;
  imageModel: string;
  size: string;
  quality: string;
  count: number;
  references: string[];
  background: Background;
  topic: string | null;
  resume: boolean;
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
}

export interface FolderView {
  /** Path as registered. */
  path: string;
  galleryRoot: string;
  available: boolean;
  isWorkDir: boolean;
}

export interface GalleryView {
  items: GalleryItem[];
  folders: FolderView[];
  truncated: boolean;
}

export type Screen = 'generate' | 'gallery' | 'connections';
