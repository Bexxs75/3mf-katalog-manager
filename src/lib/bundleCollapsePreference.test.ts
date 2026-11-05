import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadBundleCollapse, saveBundleCollapse } from './bundleCollapsePreference';

afterEach(() => { vi.restoreAllMocks(); localStorage.clear(); });

describe('bundle collapse preference', () => {
  it('is off until the user turns it on', () => {
    expect(loadBundleCollapse()).toBe(false);
  });

  it('remembers both choices', () => {
    saveBundleCollapse(true);
    expect(loadBundleCollapse()).toBe(true);
    saveBundleCollapse(false);
    expect(loadBundleCollapse()).toBe(false);
  });

  it('reads an unknown value as off', () => {
    localStorage.setItem('3mf-katalog-bundle-collapse', 'maybe');
    expect(loadBundleCollapse()).toBe(false);
  });

  it('works without readable or writable storage', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked'); });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('blocked'); });
    expect(loadBundleCollapse()).toBe(false);
    expect(() => saveBundleCollapse(true)).not.toThrow();
  });
});
