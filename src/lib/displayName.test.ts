import { expect, it } from 'vitest';
import { displayName, stripVerbatimPrefix } from './displayName';

it.each([0x061c, 0x200e, 0x200f, 0x202a, 0x202b, 0x202c, 0x202d, 0x202e, 0x2066, 0x2067, 0x2068, 0x2069])('exposes bidi control U+%s as a font-portable marker', code => {
  const control = String.fromCharCode(code);
  const raw = `part${control}txt.3mf`;
  expect(displayName(raw)).toBe(`part[U+${code.toString(16).toUpperCase().padStart(4, '0')}]txt.3mf`);
  expect(raw).toBe(`part${control}txt.3mf`);
});

it('preserves ordinary Unicode, separators, whitespace and literal markers', () => {
  const raw = 'C:\\模型\\قطعة_שלום_ä 🧩 [U+202E].3mf\n';
  expect(displayName(raw)).toBe(raw);
  expect(displayName('')).toBe('');
});

it('replaces repeated and mixed controls without affecting later calls', () => {
  expect(displayName('\u202e\u2066\u202e')).toBe('[U+202E][U+2066][U+202E]');
  expect(displayName('\u202e')).toBe('[U+202E]');
  expect(displayName(displayName('\u202e'))).toBe('[U+202E]');
});

it('hides the Windows verbatim path prefix in the display only', () => {
  expect(stripVerbatimPrefix('\\\\?\\C:\\vmt\\quelle')).toBe('C:\\vmt\\quelle');
  expect(stripVerbatimPrefix('\\\\?\\UNC\\srv\\share\\a')).toBe('\\\\srv\\share\\a');
  expect(stripVerbatimPrefix('C:\\vmt\\quelle')).toBe('C:\\vmt\\quelle');
  expect(stripVerbatimPrefix('/home/u/x')).toBe('/home/u/x');
  expect(stripVerbatimPrefix('\\\\?\\Volume{1}\\x')).toBe('\\\\?\\Volume{1}\\x');
  expect(displayName('\\\\?\\C:\\vmt\\quelle')).toBe('C:\\vmt\\quelle');
});
