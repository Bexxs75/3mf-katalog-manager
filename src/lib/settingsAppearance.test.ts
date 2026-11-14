import { expect, it } from 'vitest';
// @ts-expect-error Vitest runs on Node, but the frontend does not depend on @types/node.
import { readFileSync } from 'node:fs';
const css: string = readFileSync('src/styles/theme.css', 'utf8');
import { fr } from '../i18n/fr';
import { es } from '../i18n/es';

// Convert the theme's OKLCH values to clipped linear sRGB for WCAG luminance.
function luminance(value: string) {
  const [L, C, h] = value.split(' ').map(Number);
  const a = C * Math.cos(h * Math.PI / 180), b = C * Math.sin(h * Math.PI / 180);
  const l = (L + .3963377774 * a + .2158037573 * b) ** 3;
  const m = (L - .1055613458 * a - .0638541728 * b) ** 3;
  const s = (L - .0894841775 * a - 1.291485548 * b) ** 3;
  const rgb = [4.0767416621*l - 3.3077115913*m + .2309699292*s,
    -1.2684380046*l + 2.6097574011*m - .3413193965*s,
    -.0041960863*l - .7034186147*m + 1.707614701*s].map(v => Math.max(0, Math.min(1, v)));
  return rgb[0]*.2126 + rgb[1]*.7152 + rgb[2]*.0722;
}
it.each(['[data-app]', '[data-app="dark"]'])('keeps reset, radio and focus colors legible in %s', selector => {
  const block = css.slice(css.indexOf(`${selector} {`)).split('}')[0];
  const lum = (name: string) => luminance(block.match(new RegExp(`--${name}: oklch\\(([^)]+)\\)`))![1]);
  const contrast = (a: string, b: string) => (Math.max(lum(a), lum(b))+.05)/(Math.min(lum(a), lum(b))+.05);
  expect(contrast('crit', 'panel-2')).toBeGreaterThanOrEqual(4.5);
  for (const background of ['panel', 'panel-2', 'accent-soft']) expect(contrast('accent', background)).toBeGreaterThanOrEqual(3);
  expect(contrast('ink', 'panel')).toBeGreaterThanOrEqual(3);
});
it('uses a full viewport scrim without changing the detail overlay offset', () => {
  expect(css).toContain('.settings-panel-overlay { position: fixed; inset: 0;');
  expect(css).toContain('.settings-panel-scrim { position: absolute; inset: 0; background: var(--scrim); }');
  expect(css).toContain('.detail-panel-overlay { position: absolute; top: var(--detail-overlay-top, 0px);');
});
it('gives native radios the project accent and panel controls a separate two-pixel focus ring', () => {
  expect(css).toContain('.settings-panel input[type="radio"] { accent-color: var(--accent); }');
  expect(css).toMatch(/\.settings-panel :is\(input\[type="radio"\], input\[type="checkbox"\], \[role="switch"\]\):focus-visible \{\s*outline: 2px solid var\(--accent\);\s*outline-offset: 2px;\s*box-shadow: 0 0 0 2px var\(--panel\);/);
  expect(css).toMatch(/\.settings-panel :is\(button, summary\):focus-visible \{\s*outline: 2px solid var\(--ink\);\s*outline-offset: 2px;\s*box-shadow: 0 0 0 2px var\(--panel\);/);
});
it('lets comfort actions scroll with the content in short windows', () => {
  expect(css).toMatch(/@media \(max-height: 800px\) \{\s*\.model-panel-comfort \{ overflow-y: auto; overflow-x: hidden; \}\s*\.model-panel-comfort > \.model-panel-content \{ flex: none; overflow: visible; \}/);
});
it('uses vous in the revised instructions and retains the Spanish product name', () => {
  expect(fr.dgSwitchHint).toMatch(/^Glissez /);
  expect(fr.dgWinStep4).toContain('Si vous lancez');
  expect(fr.dgWinStep5).toBe('Lancez le slicer normalement et réessayez.');
  expect(fr.settingSaveFailedReload).toContain('Rechargez');
  expect(fr.autoTaggingBlockedNote).toContain('Vous pouvez');
  expect(es.settingsPrintersMovedHint).toContain(es.railPrinters);
});
