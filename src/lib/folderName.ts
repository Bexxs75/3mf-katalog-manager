// Mirrors `validate_new_catalog_dir_name` in the backend so the user sees the
// problem while typing. The backend check is the one that counts.

export type FolderNameProblem =
  | { kind: 'empty' }
  | { kind: 'char'; char: string }
  | { kind: 'reserved' }
  | { kind: 'trailing' }
  | { kind: 'tooLong' };

// Forbidden on Windows; rejected everywhere so a catalog keeps working after a move.
const FORBIDDEN_CHARS = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

// Windows also treats the superscript digits ¹²³ like 1, 2, 3 here.
const PORT_SUFFIXES = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '¹', '²', '³'];
const WINDOWS_RESERVED = new Set([
  'CON', 'PRN', 'AUX', 'NUL', 'CONIN$', 'CONOUT$',
  ...PORT_SUFFIXES.map((d) => `COM${d}`),
  ...PORT_SUFFIXES.map((d) => `LPT${d}`),
]);

// Most file systems allow at most 255 bytes per name component.
const MAX_NAME_BYTES = 255;

function isControl(ch: string): boolean {
  const code = ch.codePointAt(0) ?? 0;
  return code < 0x20 || (code >= 0x7f && code <= 0x9f);
}

export function folderNameProblem(name: string): FolderNameProblem | null {
  if (name.trim() === '') return { kind: 'empty' };
  for (const ch of name) {
    if (FORBIDDEN_CHARS.includes(ch)) return { kind: 'char', char: ch };
    if (isControl(ch)) {
      const hex = (ch.codePointAt(0) ?? 0).toString(16).toUpperCase().padStart(4, '0');
      return { kind: 'char', char: `U+${hex}` };
    }
  }
  if (new TextEncoder().encode(name).length > MAX_NAME_BYTES) return { kind: 'tooLong' };
  if (name === '.' || name === '..') return { kind: 'reserved' };
  if (name.endsWith('.') || name.endsWith(' ')) return { kind: 'trailing' };
  if (WINDOWS_RESERVED.has(name.split('.')[0].trimEnd().toUpperCase())) return { kind: 'reserved' };
  return null;
}
