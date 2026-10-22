// Editor choices as plain functions: which models fit, and the GenerateInput each editor action starts.

import { ENV_CHOICE } from './generateForm';
import type { GenerateInput, ModelView, ReferenceView } from './types';

/** Prompt of "AI로 투명 배경": the model redraws the image without its background. */
export const TRANSPARENT_PROMPT =
  'Keep the main subject exactly as it is (shape, colors, details, position) and remove the entire background so it is fully transparent. Do not add anything.';

export const EDIT_COUNTS = [1, 2, 3, 4] as const;

/** A model that takes the image to edit: the current one when it can, else the first that can, else ''. */
export function pickEditModel(models: ModelView[], current: string): string {
  const usable = models.filter((m) => m.references);
  return usable.find((m) => m.id === current)?.id ?? usable[0]?.id ?? '';
}

/** A model with transparent output that also takes the image: current, then gpt-image-2.5, then any; '' when none. */
export function pickTransparentModel(models: ModelView[], current: string): string {
  const usable = models.filter((m) => m.transparency && m.references);
  return (
    usable.find((m) => m.id === current)?.id ?? usable.find((m) => m.id === 'gpt-image-2.5')?.id ?? usable[0]?.id ?? ''
  );
}

interface EditBase {
  connection: string | null;
  model: string;
  quality: string;
  source: ReferenceView;
}

function base(b: EditBase): Omit<GenerateInput, 'prompt' | 'count' | 'background' | 'edit'> {
  return {
    profile: b.connection === ENV_CHOICE ? null : b.connection,
    avoid: '',
    imageModel: b.model,
    // The result keeps the source size.
    size: `${b.source.width}x${b.source.height}`,
    quality: b.quality,
    references: [b.source.path],
    fit: 'contain',
    topic: null,
    resume: false,
  };
}

/** "칠한 부분 수정": the source, the painted mask and the drawing. */
export function maskEditInput(b: EditBase, prompt: string, count: number, mask: string, strokes: string | null): GenerateInput {
  return { ...base(b), prompt: prompt.trim(), count, background: 'auto', edit: { mask, strokes } };
}

/** "AI로 투명 배경": the whole image redrawn with a transparent background. */
export function transparentInput(b: EditBase): GenerateInput {
  return { ...base(b), prompt: TRANSPARENT_PROMPT, count: 1, background: 'transparent', edit: null };
}

/** "12.3 / 170.4 MB". */
export function downloadText(done: number, total: number): string {
  const mb = (n: number) => (n / 1024 / 1024).toFixed(1);
  return `${mb(done)} / ${mb(total)} MB`;
}
