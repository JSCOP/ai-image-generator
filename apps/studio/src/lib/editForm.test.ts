import { describe, expect, it } from 'vitest';
import { downloadText, maskEditInput, pickEditModel, pickTransparentModel, transparentInput, TRANSPARENT_PROMPT } from './editForm';
import { ENV_CHOICE } from './generateForm';
import type { ModelView } from './types';

const m = (id: string, references: boolean, transparency: boolean): ModelView => ({
  id,
  provider: 'OpenAI',
  references,
  maxReferences: references ? 4 : 0,
  transparency,
});
const models = [m('grok-imagine-image-2.0', false, false), m('gemini-3.1-flash-image', true, false), m('gpt-image-2.5', true, true), m('gpt-image-2.5-sunburst', true, true)];
const source = { path: 'C:\\g\\a.png', name: 'a.png', bytes: 10, width: 1536, height: 1024 };

describe('model choice', () => {
  it('edits with a model that takes the image', () => {
    expect(pickEditModel(models, 'gemini-3.1-flash-image')).toBe('gemini-3.1-flash-image');
    expect(pickEditModel(models, 'grok-imagine-image-2.0')).toBe('gemini-3.1-flash-image');
    expect(pickEditModel([m('grok', false, false)], 'grok')).toBe('');
  });

  it('makes transparency with a transparent model, gpt-image-2.5 first', () => {
    expect(pickTransparentModel(models, 'gpt-image-2.5-sunburst')).toBe('gpt-image-2.5-sunburst');
    expect(pickTransparentModel(models, 'gemini-3.1-flash-image')).toBe('gpt-image-2.5');
    expect(pickTransparentModel(models.slice(0, 2), 'x')).toBe('');
  });
});

describe('inputs', () => {
  const b = { connection: 'local', model: 'gpt-image-2.5', quality: 'high', source };

  it('mask edit keeps the source size and sends the layers', () => {
    const input = maskEditInput(b, '  빨간 모자  ', 2, 'data:mask', null);
    expect(input).toMatchObject({
      profile: 'local',
      prompt: '빨간 모자',
      size: '1536x1024',
      count: 2,
      references: ['C:\\g\\a.png'],
      background: 'auto',
      fit: 'contain',
      topic: null,
      resume: false,
      edit: { mask: 'data:mask', strokes: null },
    });
  });

  it('AI transparency asks for a transparent background with a fixed prompt', () => {
    const input = transparentInput({ ...b, connection: ENV_CHOICE });
    expect(input.profile).toBeNull();
    expect(input.prompt).toBe(TRANSPARENT_PROMPT);
    expect(input.background).toBe('transparent');
    expect(input.edit).toBeNull();
    expect(input.count).toBe(1);
  });

  it('download progress in MB', () => {
    expect(downloadText(12.5 * 1024 * 1024, 178_648_008)).toBe('12.5 / 170.4 MB');
  });
});
