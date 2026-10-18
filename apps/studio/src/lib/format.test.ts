import { describe, expect, it } from 'vitest';
import { formatSize } from './format';

describe('formatSize', () => {
  it('shows the size with a multiplication sign', () => {
    expect(formatSize('1920x1080')).toBe('1920×1080');
  });
});
