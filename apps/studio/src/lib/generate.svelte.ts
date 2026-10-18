// Generate screen state (Svelte 5 runes). It lives at module level so a run keeps going and stays visible when the
// user switches screens (runs are cancelled only by the cancel button or by closing the window).

import * as api from './api';
import { failRun, reduce, type Card } from './run';
import {
  addReferences,
  capabilities,
  defaultConnection,
  defaultModel,
  emptyForm,
  ENV_CHOICE,
  MAX_REFERENCES,
  resumeBlocked,
  resumeFrom,
  retryInput,
  formFromItem,
  toInput,
  type GenerateForm,
  type ItemSettings,
} from './generateForm';
import { app, refreshConnections, toast, toastError } from './state.svelte';
import type { FolderView, GalleryItem, GenerateInput, ModelView, ReferenceView, RunSummary, ServiceError } from './types';

export interface RunState {
  runId: string | null;
  input: GenerateInput;
  cards: Card[];
  topic: string;
  outputDir: string;
  summary: RunSummary | null;
  /** start_generation was refused before or after `started`. */
  error: ServiceError | null;
  cancelling: boolean;
}

export const gen = $state({
  form: emptyForm() as GenerateForm,
  ready: false,
  models: [] as ModelView[],
  /** Connection the model list belongs to. */
  modelsFor: null as string | null,
  modelsLoading: false,
  modelsError: null as ServiceError | null,
  run: null as RunState | null,
  running: false,
  history: [] as GalleryItem[],
  /** Gallery folders of the last history load (which one is the work folder). */
  folders: [] as FolderView[],
  historyLoading: false,
  historyError: null as ServiceError | null,
  /** Center view: the run, or a history group key. */
  view: 'run' as string,
});

/** First visit: pick the connection, load its models and the history. Later visits keep the form. */
export function initGenerate(): void {
  if (!gen.ready) {
    gen.ready = true;
    gen.form.connection = defaultConnection(app.connections);
    void loadHistory();
  }
  syncConnection();
}

/** Keeps the chosen connection valid when the connection list changes (added, renamed, removed). */
export function syncConnection(): void {
  const view = app.connections;
  if (!view) return;
  const exists =
    gen.form.connection !== null &&
    (gen.form.connection === ENV_CHOICE ? !!view.env : view.profiles.some((p) => p.name === gen.form.connection));
  if (!exists) gen.form.connection = defaultConnection(view);
  if (gen.form.connection !== gen.modelsFor && !gen.modelsLoading) void loadModels();
}

let modelsRequest = 0;

/** Image models of the chosen connection (no paid request). */
export async function loadModels(): Promise<void> {
  const connection = gen.form.connection;
  const request = ++modelsRequest;
  gen.modelsFor = connection;
  gen.modelsError = null;
  if (connection === null) {
    gen.models = [];
    return;
  }
  gen.modelsLoading = true;
  try {
    const models = await api.listModels(connection === ENV_CHOICE ? null : connection);
    if (request !== modelsRequest) return;
    gen.models = models;
    gen.form.model = defaultModel(models, gen.form.model, app.settings?.lastModel ?? null);
    fitToModel();
  } catch (e) {
    if (request !== modelsRequest) return;
    gen.models = [];
    gen.modelsError = api.toServiceError(e);
  } finally {
    if (request === modelsRequest) gen.modelsLoading = false;
  }
}

export function chooseConnection(connection: string): void {
  if (connection === gen.form.connection && gen.modelsFor === connection) return;
  gen.form.connection = connection;
  void loadModels();
}

/** Turns off what the chosen model cannot do (transparency); extra references stay and block the button with a reason. */
export function fitToModel(): void {
  const caps = capabilities(gen.models.find((m) => m.id === gen.form.model));
  if (gen.form.transparent && !caps.transparency) gen.form.transparent = false;
}

export function referenceLimit(): number {
  const model = gen.models.find((m) => m.id === gen.form.model);
  return model ? capabilities(model).maxReferences : MAX_REFERENCES;
}

/** Checks picked or dropped files and adds the usable ones; says why the others were left out. */
export async function addReferencePaths(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  const checked = await Promise.allSettled(paths.map((p) => api.inspectReference(p)));
  const views: ReferenceView[] = [];
  const problems: string[] = [];
  for (const result of checked) {
    if (result.status === 'fulfilled') views.push(result.value);
    else problems.push(api.toServiceError(result.reason).message);
  }
  const { references, rejected } = addReferences(gen.form.references, views, referenceLimit());
  gen.form.references = references;
  for (const text of [...problems, ...rejected]) toast(text, 'warn', 7000);
}

export function removeReference(path: string): void {
  gen.form.references = gen.form.references.filter((r) => r.path !== path);
}

/** Gallery records for the history panel and the viewer's details. */
export async function loadHistory(): Promise<void> {
  gen.historyLoading = true;
  try {
    const view = await api.gallery();
    gen.history = view.items;
    gen.folders = view.folders;
    gen.historyError = null;
  } catch (e) {
    gen.historyError = api.toServiceError(e);
  } finally {
    gen.historyLoading = false;
  }
}

function rememberChoice(input: GenerateInput): void {
  const settings = app.settings;
  if (!settings) return;
  const next = { ...settings, lastModel: input.imageModel, lastProfile: input.profile };
  if (next.lastModel === settings.lastModel && next.lastProfile === settings.lastProfile) return;
  // Only a convenience for the next start: a failed save is not worth an error.
  api.saveSettings(next).then(
    () => {
      app.settings = next;
    },
    () => {},
  );
}

/** Starts a run with the form; the button is disabled while toInput reports reasons, so a refusal here is rare. */
export async function startRun(): Promise<void> {
  if (gen.running) return;
  const result = toInput(gen.form, gen.models);
  if (!result.ok) {
    toast(result.errors[0], 'warn');
    return;
  }
  await runWith(result.input, true);
}

/** "실패한 장 다시 만들기": the last run's settings resumed on its topic, so only the missing images are made. */
export async function retryFailed(): Promise<void> {
  const last = gen.run;
  if (gen.running || !last || !last.topic) return;
  await runWith(retryInput(last.input, last.topic), false);
}

async function runWith(input: GenerateInput, fromForm: boolean): Promise<void> {
  gen.running = true;
  gen.view = 'run';
  gen.run = { runId: null, input, cards: [], topic: input.topic ?? '', outputDir: '', summary: null, error: null, cancelling: false };
  const run = gen.run;
  rememberChoice(input);
  try {
    const summary = await api.startGeneration(input, (ev) => {
      if (ev.event === 'started') {
        run.runId = ev.data.runId;
        run.topic = ev.data.topic;
        run.outputDir = ev.data.outputDir;
        // Cancel was pressed before the run had an id.
        if (run.cancelling) void api.cancelGeneration(ev.data.runId).catch(() => {});
      }
      run.cards = reduce(run.cards, ev);
    });
    run.summary = summary;
    // On the run view the header already sums the run up (and a toast would sit over its buttons).
    const seen = app.screen === 'generate' && gen.view === 'run';
    if (!seen) {
      if (summary.cancelled) toast('생성을 취소했습니다. 끝난 이미지는 그대로 남습니다.', 'info');
      else if (summary.ok) toast(`${summary.outputs.length}장을 만들었습니다.`, 'ok');
      else toast(`${summary.outputs.length}장 완료, ${summary.failures.length}장 실패`, 'warn');
    }
    for (const warning of summary.warnings) toast(warning, 'warn', 8000);
  } catch (e) {
    const error = api.toServiceError(e);
    run.error = error;
    run.cards = failRun(run.cards, error);
    toastError(error);
  } finally {
    run.cancelling = false;
    gen.running = false;
    if (fromForm && gen.form.resume) {
      // A finished resume run is done with that topic; the next start makes a new one.
      gen.form.resume = false;
      gen.form.topic = null;
    }
    void loadHistory();
    void refreshConnections();
  }
}

export async function cancelRun(): Promise<void> {
  const run = gen.run;
  if (!run || !gen.running || run.cancelling) return;
  run.cancelling = true;
  if (!run.runId) return; // not started yet: the `started` handler cancels it
  try {
    await api.cancelGeneration(run.runId);
  } catch (e) {
    run.cancelling = false;
    toastError(api.toServiceError(e));
  }
}

async function applySettings(settings: ItemSettings & { count?: number }): Promise<void> {
  const f = gen.form;
  f.prompt = settings.prompt;
  f.avoid = settings.avoid;
  f.size = settings.size;
  f.customSize = settings.customSize;
  f.quality = settings.quality;
  f.transparent = settings.transparent;
  f.topic = settings.topic;
  f.resume = settings.resume;
  if (settings.count) f.count = settings.count;
  f.model = settings.model;
  if (gen.models.length > 0 && !gen.models.some((m) => m.id === settings.model)) {
    toast(`이 연결의 목록에 없는 모델입니다: ${settings.model}`, 'warn', 7000);
  }
  f.references = [];
  if (settings.referencePaths.length > 0) await addReferencePaths(settings.referencePaths);
}

/** "이 설정 다시 쓰기": a new run with a record's settings. */
export function reuseItem(item: GalleryItem): Promise<void> {
  gen.view = 'run';
  return applySettings(formFromItem(item));
}

/** "이어서 생성": the same topic with resume, so finished images are kept and only the rest are made. */
export function resumeGroup(items: GalleryItem[]): Promise<void> {
  if (items.length === 0) return Promise.resolve();
  const blocked = resumeBlocked(items[0].folder, gen.folders);
  if (blocked) {
    toast(blocked, 'warn', 7000);
    return Promise.resolve();
  }
  gen.view = 'run';
  return applySettings(resumeFrom(items));
}

export function stopResume(): void {
  gen.form.resume = false;
  gen.form.topic = null;
}
