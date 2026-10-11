import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

// The details panel paints an absolutely positioned hatch layer behind the viewer.
// A static element would sit below that layer and never receive clicks.
describe('plate selector layering', () => {
  it('is positioned so it stays above the details panel background', () => {
    const css = readFileSync('src/styles/theme.css', 'utf8');
    const rule = css.match(/^\.plate-selector \{[^}]*\}/m)?.[0] ?? '';
    expect(rule).toMatch(/position:\s*relative/);
  });
});
