import { describe, expect, it } from 'vitest';
import { splitFileName } from './fileName';

describe('splitFileName', () => {
  it('splits a normal file name into base and extension', () => {
    expect(splitFileName('Adapter.3mf')).toEqual({ base: 'Adapter', extension: '.3mf' });
    expect(splitFileName('model.stl')).toEqual({ base: 'model', extension: '.stl' });
  });

  it('uses the last dot when the base name itself contains dots', () => {
    expect(splitFileName('v1.2.model.3mf')).toEqual({ base: 'v1.2.model', extension: '.3mf' });
  });

  it('treats a name with no dot as having no extension', () => {
    expect(splitFileName('README')).toEqual({ base: 'README', extension: '' });
  });

  it('treats a leading dot (hidden file) as part of the base, not an extension', () => {
    expect(splitFileName('.gitignore')).toEqual({ base: '.gitignore', extension: '' });
  });
});
