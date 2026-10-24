import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

function readSources(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? readSources(path) : /\.(tsx|ts)$/.test(path) && !/\.test\./.test(path) ? [[path, readFileSync(path, 'utf8')]] : [];
  });
}
const css = readFileSync('src/styles/theme.css', 'utf8');

// Every text field gets its focus indicator from one rule in theme.css: the
// system ring is switched off and the field's own border turns accent. Keeping
// this central protects the fields that have no focus classes of their own.
describe('field focus contract', () => {
  it('replaces the system focus ring of text fields, textareas and selects with the accent border', () => {
    const rule = css.match(/^input:where\(:not\([^)]*\)\):focus, textarea:focus, select:focus \{[^}]*\}/m)?.[0] ?? '';
    expect(rule).toMatch(/outline:\s*none/);
    expect(rule).toMatch(/border-color:\s*var\(--accent\)/);
    expect(rule).toMatch(/\[type="checkbox"\]/);
    expect(rule).toMatch(/\[type="radio"\]/);
  });
  it('gives checkboxes and radios a visible accent ring for keyboard users', () => {
    const rule = css.match(/^input\[type="checkbox"\]:focus-visible, input\[type="radio"\]:focus-visible \{[^}]*\}/m)?.[0] ?? '';
    expect(rule).toMatch(/outline:\s*2px solid var\(--accent\)/);
  });
  it('never relies on outline-0, which only sets the width and leaves the WebKit ring visible', () => {
    for (const [path, source] of readSources('src')) {
      expect(source, path).not.toMatch(/(^|[\s"'`])outline-0([\s"'`]|$)/);
    }
  });
});
