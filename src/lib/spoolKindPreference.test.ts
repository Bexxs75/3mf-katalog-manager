import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadSpoolKind, saveSpoolKind } from './spoolKindPreference';

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.removeItem('3mf-katalog-filament-kind');
});

describe('spoolKindPreference', () => {
  it('defaults to filament and remembers resin', () => {
    expect(loadSpoolKind()).toBe('filament');
    saveSpoolKind('resin');
    expect(loadSpoolKind()).toBe('resin');
  });

  it('ignores unknown stored values', () => {
    localStorage.setItem('3mf-katalog-filament-kind', 'wood');
    expect(loadSpoolKind()).toBe('filament');
  });

  it('survives a storage that throws', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(loadSpoolKind()).toBe('filament');
    expect(() => saveSpoolKind('resin')).not.toThrow();
  });
});
