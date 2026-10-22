import { afterEach, describe, expect, it, vi } from 'vitest';
import { copyText, promptText } from './clipboard';

describe('promptText', () => {
  it('is the prompt as sent: the avoid part on its own "Avoid:" line', () => {
    expect(promptText('  cozy reading nook  ', '')).toBe('cozy reading nook');
    expect(promptText('cozy reading nook', ' text, watermark ')).toBe('cozy reading nook\nAvoid: text, watermark');
    // A record prompt already carries its Avoid line.
    expect(promptText('mug\nAvoid: text')).toBe('mug\nAvoid: text');
  });
});

describe('copyText', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('writes to the clipboard and reports success', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { clipboard: { writeText } });
    expect(await copyText('hello')).toBe(true);
    expect(writeText).toHaveBeenCalledWith('hello');
  });

  it('reports failure instead of throwing when the clipboard refuses', async () => {
    vi.stubGlobal('navigator', { clipboard: { writeText: vi.fn().mockRejectedValue(new Error('denied')) } });
    expect(await copyText('hello')).toBe(false);
    vi.stubGlobal('navigator', {});
    expect(await copyText('hello')).toBe(false);
  });
});
