import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
const css = readFileSync('src/styles/theme.css', 'utf8');

// OKLCH -> linear sRGB (CSS Color 4 matrices). Clip to the display gamut,
// encode sRGB, then apply the WCAG relative luminance transfer function.
function luminance(value) {
  const match = value.match(/oklch\(([\d.]+) ([\d.]+) ([\d.]+)\)/);
  if (!match) throw new Error(`Expected OKLCH, received ${value}`);
  const [, lightness, chroma, hue] = match.map(Number);
  const a = chroma * Math.cos(hue * Math.PI / 180);
  const b = chroma * Math.sin(hue * Math.PI / 180);
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const linear = [4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s];
  const srgb = linear.map(v => { const c = Math.max(0, Math.min(1, v)); return c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055; });
  const wcag = srgb.map(v => v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
  return wcag[0] * 0.2126 + wcag[1] * 0.7152 + wcag[2] * 0.0722;
}
function contrast(a, b) {
  const x = luminance(a), y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
const pairs = [['ink-3', 'bg'], ['ink-3', 'panel'], ['ink-3', 'panel-2'],
  ['accent', 'accent-soft'], ['accent', 'bg'], ['good', 'good-soft'], ['good', 'panel'],
  ['crit', 'crit-soft'], ['crit', 'panel'], ['warn', 'warn-soft'], ['warn', 'panel']];
for (const [theme, selector] of [['light', '[data-app]'], ['dark', '[data-app="dark"]']]) {
  describe(`${theme} text contrast`, () => {
    const block = css.slice(css.indexOf(`${selector} {`)).split('}')[0];
    const tokens = Object.fromEntries(Array.from(block.matchAll(/--([\w-]+):\s*(oklch\([^;]+\));/g), match => [match[1], match[2]]));
    it.each(pairs)('%s on %s is at least 4.5:1', (foreground, background) => {
      expect(contrast(tokens[foreground], tokens[background])).toBeGreaterThanOrEqual(4.5);
    });
  });
}
it('checks the luminance calculation against black and white', () => {
  expect(contrast('oklch(0 0 0)', 'oklch(1 0 0)')).toBeCloseTo(21, 5);
});
