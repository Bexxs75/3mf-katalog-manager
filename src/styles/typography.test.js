import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

function readSources(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? readSources(path) : /\.(tsx|ts|css)$/.test(path) ? [[path, readFileSync(path, 'utf8')]] : [];
  });
}
const sources = Object.fromEntries(readSources('src'));
// No exceptions: dimensions and prose punctuation are outside the symbol pattern.
const exceptions = {};

describe('typography source contract', () => {
  for (const [path, source] of Object.entries(sources)) {
    if (/\.test\./.test(path)) continue;
    it(path, () => {
      if (exceptions[path]) return;
      expect(source).not.toMatch(/font-mono-ui|text-\[\d+(?:\.\d+)?px\]/);
      const families = source.match(/font-family\s*:[^;}]+/g) ?? [];
      if (path.endsWith('/theme.css')) {
        expect(families.filter(family => /Mono|monospace/.test(family))).toHaveLength(1);
        expect(source).toMatch(/\.font-code[^{}]*\{[^}]*font-family: 'IBM Plex Mono', monospace/);
      } else {
        expect(families.join('\n')).not.toMatch(/Mono|monospace/);
      }
      if (path.endsWith('.tsx') && !path.includes('/i18n/')) expect(source).not.toMatch(/[✓✕▾▴▸♥♡⌕✎＋⇄⚠📦⚖]/u);
      if (path.endsWith('.css')) expect(source).not.toMatch(/font-size:\s*\d+(?:\.\d+)?px/);
    });
  }
});
