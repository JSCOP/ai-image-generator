// Generate screen state (Svelte 5 runes). It lives at module level so a run keeps going and stays visible when the
// user switches screens (runs are cancelled only by the cancel button or by closing the window).

import * as api from './api';
import { copyText, promptText } from './clipboard';
import { failRun, reduce, type Card } from './run';
import {
  addReferences,
  capabilities,
  defaultConnection,
  defaultModel,
  DEFAULT_TEXT_MODEL,
  emptyForm,
  ENV_CHOICE,
  MAX_REFERENCES,
  resumeBlocked,
  resumeFrom,
  retryInput,
  formFromItem,
  pickTextModel,
  toInput,
  type GenerateForm,
  type ItemSettings,
} from './generateForm';
import { app, navigate, refreshConnections, toast, toastError } from './state.svelte';
import type { EnhanceView, FolderView, GalleryItem, GenerateInput, ModelView, ReferenceView, RunSummary, ServiceError } from './types';

export interface RunState {
  /** Local key for lists (the run id arrives later with `started`). */
  key: string;
  runId: string | null;
  /** Still running (no summary or error yet). */
  running: boolean;
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
  /** Runs of this session, newest first; several can run at once. */
  runs: [] as RunState[],
  history: [] as GalleryItem[],
  /** Gallery folders of the last history load (which one is the work folder). */
  folders: [] as FolderView[],
  historyLoading: false,
  historyError: null as ServiceError | null,
  /** Center view: the run, or a history group key. */
  view: 'run' as string,
  /** "영어로 다듬기": chat model, its list, the preview waiting for "적용" and the prompt before the last apply. */
  enhance: {
    model: '',
    models: [] as string[],
    busy: false,
    result: null as EnhanceView | null,
    error: null as ServiceError | null,
    undo: null as string | null,
  },
});

/** First visit: pick the connection, load its models and the history. Later visits keep the form. */
export function initGenerate(): void {
  if (!gen.ready) {
    gen.ready = true;
    gen.form.connection = defaultConnection(app.connections);
    gen.enhance.model = app.settings?.enhanceModel || DEFAULT_TEXT_MODEL;
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
  void loadTextModels(connection);
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

let textModelsRequest = 0;

/** Chat models of the connection for "영어로 다듬기"; a failure only leaves the list empty (the saved model stays). */
async function loadTextModels(connection: string): Promise<void> {
  const request = ++textModelsRequest;
  let models: string[] = [];
  try {
    models = await api.textModels(connection === ENV_CHOICE ? null : connection);
  } catch {
    models = [];
  }
  if (request !== textModelsRequest) return;
  gen.enhance.models = models;
  gen.enhance.model = pickTextModel(models, gen.enhance.model, app.settings?.enhanceModel ?? null);
}

let enhanceRequest = 0;

/** Asks the chat model for an English prompt; the answer waits in `gen.enhance.result` until applied or discarded. */
export async function enhancePrompt(): Promise<void> {
  const e = gen.enhance;
  const connection = gen.form.connection;
  if (e.busy || !connection || !gen.form.prompt.trim()) return;
  const request = ++enhanceRequest;
  e.busy = true;
  e.error = null;
  e.result = null;
  const model = e.model.trim() || pickTextModel(e.models, '', app.settings?.enhanceModel ?? null);
  try {
    const result = await api.enhancePrompt({
      profile: connection === ENV_CHOICE ? null : connection,
      prompt: gen.form.prompt,
      avoid: gen.form.avoid,
      imageModel: gen.form.model || null,
      model,
    });
    if (request !== enhanceRequest) return;
    e.result = result;
    rememberEnhanceModel(model);
  } catch (err) {
    if (request === enhanceRequest) e.error = api.toServiceError(err);
  } finally {
    if (request === enhanceRequest) e.busy = false;
  }
}

/** Puts the English prompt in the prompt box; the previous text can come back with undoEnhance. */
export function applyEnhanced(): void {
  const result = gen.enhance.result;
  if (!result) return;
  gen.enhance.undo = gen.form.prompt;
  // The first own text stays the original through repeated refinements.
  gen.form.originalPrompt ??= gen.form.prompt.trim() || null;
  gen.form.prompt = result.prompt;
  gen.enhance.result = null;
}

export function discardEnhanced(): void {
  enhanceRequest++;
  gen.enhance.busy = false;
  gen.enhance.result = null;
  gen.enhance.error = null;
}

export function undoEnhance(): void {
  if (gen.enhance.undo === null) return;
  gen.form.prompt = gen.enhance.undo;
  gen.form.originalPrompt = null;
  gen.enhance.undo = null;
}

function rememberEnhanceModel(model: string): void {
  const settings = app.settings;
  if (!settings || settings.enhanceModel === model) return;
  const next = { ...settings, enhanceModel: model };
  // A convenience for the next start, like rememberChoice.
  api.saveSettings(next).then(
    () => {
      app.settings = next;
    },
    () => {},
  );
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
  mergeReferences(await Promise.allSettled(paths.map((p) => api.inspectReference(p))));
}

function dataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

/** Ctrl+V: pasted images are saved into the work folder first (a reference is a file), then added like picked ones. */
export async function pasteReferenceFiles(files: File[]): Promise<void> {
  if (files.length === 0) return;
  mergeReferences(await Promise.allSettled(files.map(async (f) => api.pasteReference(await dataUrl(f)))));
}

function mergeReferences(checked: PromiseSettledResult<ReferenceView>[]): void {
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
    const view = await api.gallery(undefined, false);
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

/** Finished runs kept on screen besides the running ones. */
export const KEEP_FINISHED_RUNS = 6;

export function runningCount(): number {
  return gen.runs.filter((r) => r.running).length;
}

let runKeys = 0;

/** Starts a run with the form, also while other runs go on; the button is disabled while toInput reports reasons. */
export async function startRun(): Promise<void> {
  const result = toInput(gen.form, gen.models);
  if (!result.ok) {
    toast(result.errors[0], 'warn');
    return;
  }
  await runWith(result.input, true);
}

/** "실패한 장 다시 만들기": that run's settings resumed on its topic, so only the missing images are made. */
export async function retryFailed(run: RunState): Promise<void> {
  if (run.running || !run.topic) return;
  if (gen.runs.some((r) => r.running && r.topic === run.topic)) {
    toast('이 주제는 이미 다시 만드는 중입니다.', 'info');
    return;
  }
  await runWith(retryInput(run.input, run.topic), false);
}

/** A run the editor built (mask edit, AI transparency); it shows in the run list like any other. */
export function runInput(input: GenerateInput): Promise<void> {
  return runWith(input, false);
}

/** Removes a finished run from the screen (its images stay in the gallery). */
export function dismissRun(run: RunState): void {
  if (run.running) return;
  gen.runs = gen.runs.filter((r) => r.key !== run.key);
}

async function runWith(input: GenerateInput, fromForm: boolean): Promise<void> {
  gen.view = 'run';
  gen.runs.unshift({
    key: `local-${++runKeys}`,
    runId: null,
    running: true,
    input,
    cards: [],
    topic: input.topic ?? '',
    outputDir: '',
    summary: null,
    error: null,
    cancelling: false,
  });
  // The reactive copy: mutations through it update the screen.
  const run = gen.runs[0];
  // Older finished runs make room; running ones always stay.
  let finished = 0;
  gen.runs = gen.runs.filter((r) => r.running || ++finished <= KEEP_FINISHED_RUNS);
  if (fromForm && gen.form.resume) {
    // The run carries the topic now; the next start makes a new one.
    gen.form.resume = false;
    gen.form.topic = null;
  }
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
    run.running = false;
    void loadHistory();
    void refreshConnections();
  }
}

export async function cancelRun(run: RunState): Promise<void> {
  if (!run.running || run.cancelling) return;
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
  f.originalPrompt = settings.originalPrompt;
  f.avoid = settings.avoid;
  f.size = settings.size;
  f.customSize = settings.customSize;
  f.quality = settings.quality;
  f.transparent = settings.transparent;
  f.fit = settings.fit;
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

/** "원래 요청으로 다시 다듬기": that record's settings with the user's own request back in the prompt box. */
export async function refineFromOriginal(item: GalleryItem): Promise<void> {
  if (!item.originalPrompt) return;
  navigate('generate');
  await reuseItem(item);
  gen.form.prompt = item.originalPrompt;
  gen.form.originalPrompt = null;
  gen.enhance.undo = null;
  gen.enhance.result = null;
}

/** The prompt box was emptied: a stale original must not ride along with the next, new prompt. */
export function promptEdited(): void {
  if (!gen.form.prompt.trim()) gen.form.originalPrompt = null;
}

/** "프롬프트 복사": the prompt as sent (with its Avoid line) to the clipboard. */
export async function copyPrompt(prompt: string, avoid = ''): Promise<void> {
  if (await copyText(promptText(prompt, avoid))) toast('프롬프트를 복사했습니다.', 'ok', 2500);
  else toast('클립보드에 복사하지 못했습니다.', 'warn');
}
