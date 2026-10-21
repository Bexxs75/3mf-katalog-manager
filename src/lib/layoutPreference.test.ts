import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadLayout, saveLayout } from './layoutPreference';

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.removeItem('3mf-katalog-filament-layout');
});

describe('layoutPreference', () => {
  it('defaults to dashboard and remembers list', () => {
    expect(loadLayout()).toBe('dashboard');
    saveLayout('list');
    expect(loadLayout()).toBe('list');
    saveLayout('dashboard');
    expect(loadLayout()).toBe('dashboard');
  });

  it('ignores unknown stored values', () => {
    localStorage.setItem('3mf-katalog-filament-layout', 'wood');
    expect(loadLayout()).toBe('dashboard');
  });

  it('survives a storage that throws', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(loadLayout()).toBe('dashboard');
    expect(() => saveLayout('list')).not.toThrow();
  });
});
