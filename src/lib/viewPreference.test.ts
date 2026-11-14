import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loadView, saveView } from './viewPreference';

beforeEach(() => localStorage.removeItem('3mf-katalog-view'));
afterEach(() => {
  vi.restoreAllMocks();
  localStorage.removeItem('3mf-katalog-view');
});

describe('viewPreference', () => {
  it('defaults to grid without a stored preference', () => {
    expect(loadView()).toBe('grid');
  });

  it.each(['grid', 'groupedGrid', 'groupedList'] as const)('remembers %s', view => {
    saveView(view);
    expect(localStorage.getItem('3mf-katalog-view')).toBe(view);
    expect(loadView()).toBe(view);
  });

  it('falls back to grid for an invalid value', () => {
    localStorage.setItem('3mf-katalog-view', 'list');
    expect(loadView()).toBe('grid');
  });

  it('survives a blocked localStorage getter', () => {
    vi.spyOn(window, 'localStorage', 'get').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(loadView()).toBe('grid');
    expect(() => saveView('groupedList')).not.toThrow();
  });

  it('survives storage method failures', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked'); });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('full'); });
    expect(loadView()).toBe('grid');
    expect(() => saveView('groupedGrid')).not.toThrow();
  });
});
