import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadGroupMode, saveGroupMode } from './groupModePreference';

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.removeItem('3mf-katalog-filament-group-mode');
});

describe('groupModePreference', () => {
  it('defaults to grouped and remembers single', () => {
    expect(loadGroupMode()).toBe('grouped');
    saveGroupMode('single');
    expect(loadGroupMode()).toBe('single');
    saveGroupMode('grouped');
    expect(loadGroupMode()).toBe('grouped');
  });

  it('ignores unknown stored values', () => {
    localStorage.setItem('3mf-katalog-filament-group-mode', 'tiles');
    expect(loadGroupMode()).toBe('grouped');
  });

  it('survives a storage that throws', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(loadGroupMode()).toBe('grouped');
    expect(() => saveGroupMode('single')).not.toThrow();
  });
});
