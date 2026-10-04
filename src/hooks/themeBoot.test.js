import { expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const html = read('../../index.html');
const script = read('../../public/theme-init.js');
const bootCss = read('../../public/theme-boot.css');
const themeCss = read('../styles/theme.css');

it('loads blocking external theme assets in head before the React module', () => {
  const headEnd = html.indexOf('</head>');
  const scriptIndex = html.indexOf('<script src="/theme-init.js"></script>');
  const styleIndex = html.indexOf('<link rel="stylesheet" href="/theme-boot.css" />');
  const moduleIndex = html.indexOf('type="module"');
  expect(scriptIndex).toBeGreaterThan(0);
  expect(styleIndex).toBeGreaterThan(0);
  expect(scriptIndex).toBeLessThan(headEnd);
  expect(styleIndex).toBeLessThan(headEnd);
  expect(scriptIndex).toBeLessThan(moduleIndex);
  expect(styleIndex).toBeLessThan(moduleIndex);
  expect(script).not.toMatch(/\bimport\b/);
});

it.each([
  ['light', true, 'light'], ['dark', false, 'dark'],
  ['system', true, 'dark'], ['system', false, 'light'],
  [null, true, 'dark'], [null, false, 'light'], ['', true, 'dark'],
])('resolves stored %s and system dark=%s to %s', (stored, dark, expected) => {
  const setAttribute = vi.fn();
  const getItem = vi.fn().mockReturnValue(stored);
  const matchMedia = vi.fn().mockReturnValue({matches: dark});
  new Function('localStorage', 'window', 'document', script)({getItem}, {matchMedia}, {documentElement: {setAttribute}});
  expect(getItem).toHaveBeenCalledWith('3mf-katalog-theme');
  expect(setAttribute).toHaveBeenCalledExactlyOnceWith('data-app', expected);
});

it.each([true, false])('uses system preference if storage is unavailable (dark=%s)', dark => {
  const setAttribute = vi.fn();
  new Function('localStorage', 'window', 'document', script)(
    {getItem: () => { throw new Error('Storage denied'); }},
    {matchMedia: () => ({matches: dark})}, {documentElement: {setAttribute}});
  expect(setAttribute).toHaveBeenCalledExactlyOnceWith('data-app', dark ? 'dark' : 'light');
});

it('keeps boot background and text colors in sync with theme.css', () => {
  const blocks = ['[data-app]', '[data-app="dark"]'].map(selector => themeCss.slice(themeCss.indexOf(`${selector} {`)).split('}')[0]);
  for (const block of blocks) {
    for (const token of ['bg', 'ink']) {
      const color = block.match(new RegExp(`--${token}: (oklch\\([^;]+\\));`))[1];
      expect(bootCss).toContain(`${token === 'bg' ? 'background' : 'color'}: ${color}`);
    }
  }
});
