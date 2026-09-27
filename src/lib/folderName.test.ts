import { describe, expect, it } from 'vitest';
import { folderNameProblem } from './folderName';

describe('folderNameProblem', () => {
  it('accepts ordinary names', () => {
    for (const name of ['3D-Katalog', 'Meine Modelle', 'Catálogo 3D', '.versteckt', 'a.b']) {
      expect(folderNameProblem(name)).toBeNull();
    }
  });

  it('reports empty names', () => {
    expect(folderNameProblem('')).toEqual({ kind: 'empty' });
    expect(folderNameProblem('   ')).toEqual({ kind: 'empty' });
  });

  it('reports the first forbidden character', () => {
    expect(folderNameProblem('3D:Katalog')).toEqual({ kind: 'char', char: ':' });
    for (const ch of ['/', '\\', '*', '?', '"', '<', '>', '|']) {
      expect(folderNameProblem(`a${ch}b`)).toEqual({ kind: 'char', char: ch });
    }
    expect(folderNameProblem('a\u0007b')).toEqual({ kind: 'char', char: 'U+0007' });
  });

  it('reports reserved names and trailing dots or spaces', () => {
    expect(folderNameProblem('.')).toEqual({ kind: 'reserved' });
    expect(folderNameProblem('..')).toEqual({ kind: 'reserved' });
    expect(folderNameProblem('CON')).toEqual({ kind: 'reserved' });
    expect(folderNameProblem('nul.txt')).toEqual({ kind: 'reserved' });
    for (const name of ['CONIN$', 'conout$.log', 'COM¹', 'lpt³', 'Com9']) {
      expect(folderNameProblem(name)).toEqual({ kind: 'reserved' });
    }
    expect(folderNameProblem('CONSOLE')).toBeNull();
    expect(folderNameProblem('Katalog.')).toEqual({ kind: 'trailing' });
    expect(folderNameProblem('Katalog ')).toEqual({ kind: 'trailing' });
  });

  it('limits names to 255 bytes, not characters', () => {
    expect(folderNameProblem('a'.repeat(255))).toBeNull();
    expect(folderNameProblem('a'.repeat(256))).toEqual({ kind: 'tooLong' });
    expect(folderNameProblem('ä'.repeat(128))).toEqual({ kind: 'tooLong' });
  });
});
