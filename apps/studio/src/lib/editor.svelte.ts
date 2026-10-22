// Editor state (Svelte 5 runes): the image being edited and the actions that start from it.
// Mask edits and AI transparency become ordinary runs on the generate screen; the local cut-out saves a new image.

import * as api from './api';
import { maskEditInput, pickEditModel, pickTransparentModel, transparentInput } from './editForm';
import { loadGallery } from './gallery.svelte';
import { gen, initGenerate, runInput } from './generate.svelte';
import { navigate, toast, toastError } from './state.svelte';
import type { ReferenceView } from './types';

export const editor = $state({
  source: null as ReferenceView | null,
  /** The local cut-out is running. */
  busy: false,
  /** Model download progress of the first cut-out. */
  download: null as { done: number; total: number } | null,
});

/** Opens the editor on an image (a result, a gallery record or a reference); its size and thumbnail come from the app. */
export async function openEditor(path: string): Promise<void> {
  try {
    editor.source = await api.inspectReference(path);
    // The model lists come from the generate screen's connection.
    initGenerate();
  } catch (e) {
    toastError(api.toServiceError(e));
  }
}

export function closeEditor(): void {
  if (editor.busy) return;
  editor.source = null;
}

function editBase(model: string) {
  const source = editor.source;
  if (!source) return null;
  return { connection: gen.form.connection, model, quality: gen.form.quality, source };
}

export function editModel(): string {
  return pickEditModel(gen.models, gen.form.model);
}

export function transparentModel(): string {
  return pickTransparentModel(gen.models, gen.form.model);
}

/** "칠한 부분 수정": a run on the generate screen. */
export function startMaskEdit(prompt: string, model: string, count: number, mask: string, strokes: string | null): void {
  const b = editBase(model);
  if (!b) return;
  editor.source = null;
  navigate('generate');
  void runInput(maskEditInput(b, prompt, count, mask, strokes));
}

/** "AI로 투명 배경": a run with a transparent background on the generate screen. */
export function startAiTransparent(model: string): void {
  const b = editBase(model);
  if (!b) return;
  editor.source = null;
  navigate('generate');
  void runInput(transparentInput(b));
}

/** "내 PC에서 배경 제거": saves a transparent copy, then shows it in the editor. */
export async function cutOutLocally(): Promise<void> {
  const source = editor.source;
  if (!source || editor.busy) return;
  editor.busy = true;
  editor.download = null;
  try {
    const result = await api.removeBackground(source.path, (ev) => {
      editor.download = ev.event === 'downloading' ? ev.data : null;
    });
    toast(`배경을 지웠습니다. 완전 투명 ${(result.transparentRatio * 100).toFixed(1)}% · 원본은 그대로입니다.`, 'ok', 5000);
    editor.source = await api.inspectReference(result.output);
    void loadGallery();
  } catch (e) {
    toastError(api.toServiceError(e));
  } finally {
    editor.busy = false;
    editor.download = null;
  }
}
