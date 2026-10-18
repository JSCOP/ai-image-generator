/// <reference types="node" />
// Contrast of the design tokens (WCAG AA: 4.5:1 for text, 3:1 for focus indicators).
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const css = readFileSync(fileURLToPath(new URL('./theme.css', import.meta.url)), 'utf8');
const root = css.slice(css.indexOf(':root'), css.indexOf('}', css.indexOf(':root')));

function token(name: string): string {
  const m = new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})\\s*;`).exec(root);
  if (!m) throw new Error(`no hex token --${name}`);
  return m[1].toLowerCase();
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const surfaces = ['bg', 'panel', 'panel-2'];

describe('theme tokens', () => {
  it('--faint and --muted text reach 4.5:1 on every panel', () => {
    for (const text of ['faint', 'muted', 'text']) {
      for (const s of surfaces) expect(contrast(token(text), token(s)), `${text} on ${s}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('the focus ring is a solid colour of at least 3:1, unlike the selected (accent) border', () => {
    const ring = token('focus-ring');
    for (const s of surfaces) expect(contrast(ring, token(s)), `focus ring on ${s}`).toBeGreaterThanOrEqual(3);
    expect(ring).not.toBe(token('accent'));
    expect(contrast(ring, token('accent'))).toBeGreaterThanOrEqual(1.8);
    // An outline with an offset: scoped box-shadow rules (selected chips, kind cards) cannot hide it.
    expect(css).toMatch(/:focus-visible\s*\{[^}]*outline:\s*2px solid var\(--focus-ring\);[^}]*outline-offset:\s*2px;/);
  });
});
