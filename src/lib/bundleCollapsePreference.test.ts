import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadBundleCollapse, saveBundleCollapse } from './bundleCollapsePreference';

afterEach(() => { vi.restoreAllMocks(); localStorage.clear(); });

describe('bundle collapse preference', () => {
  it('is on until the user turns it off', () => {
    expect(loadBundleCollapse()).toBe(true);
  });

  it('remembers both choices', () => {
    saveBundleCollapse(true);
    expect(loadBundleCollapse()).toBe(true);
    saveBundleCollapse(false);
    expect(loadBundleCollapse()).toBe(false);
  });

  it('keeps a saved off, and reads an unknown value as the default', () => {
    localStorage.setItem('3mf-katalog-bundle-collapse', 'off');
    expect(loadBundleCollapse()).toBe(false);
    localStorage.setItem('3mf-katalog-bundle-collapse', 'maybe');
    expect(loadBundleCollapse()).toBe(true);
  });

  it('works without readable or writable storage', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked'); });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('blocked'); });
    expect(loadBundleCollapse()).toBe(true);
    expect(() => saveBundleCollapse(true)).not.toThrow();
  });
});
