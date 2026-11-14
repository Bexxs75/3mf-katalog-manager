import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { clampDetailPanelWidth as clamp, loadDetailPanelWidth as load, saveDetailPanelWidth as save } from './detailPanelWidthPreference';
const key = '3mf-katalog-detail-width';
beforeEach(() => localStorage.clear());
afterEach(() => vi.restoreAllMocks());
it('loads, saves and removes the preference', () => {
  expect(load()).toBeNull();
  save(456); expect(load()).toBe(456);
  save(null); expect(localStorage.getItem(key)).toBeNull();
});
it.each(['', 'text', '-1', 'NaN', 'Infinity', '299', '721'])('rejects invalid storage %s', value => {
  localStorage.setItem(key, value); expect(load()).toBeNull();
});
it.each([NaN, -1, 299, 721])('does not save invalid width %s', value => {
  save(value); expect(load()).toBeNull();
});
it('tolerates blocked reads, writes and removal', () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw Error('blocked'); });
  expect(load()).toBeNull();
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw Error('blocked'); });
  expect(() => save(400)).not.toThrow();
  vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => { throw Error('blocked'); });
  expect(() => save(null)).not.toThrow();
});
it.each([[200, 1400, 300], [800, 1400, 720], [720, 1000, 600], [720, 700, 380], [600, 620, 300], [600, 400, 300], [336, 0, 300], [500, 1200, 500]])('clamps %s in %s to %s', (px, available, expected) => {
  expect(clamp(px, available)).toBe(expected);
});
