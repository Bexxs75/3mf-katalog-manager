import { act, renderHook } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { useFolderExpansion } from './useFolderExpansion';

beforeEach(() => localStorage.clear());

it('toggles, expands idempotently, replaces all and persists across mounts', () => {
  const { result, unmount } = renderHook(useFolderExpansion);
  expect(result.current.expanded.size).toBe(0);
  act(() => result.current.toggle('a'));
  act(() => result.current.expand('a'));
  expect(result.current.isExpanded('a')).toBe(true);
  act(() => result.current.toggle('a'));
  expect(result.current.isExpanded('a')).toBe(false);
  act(() => result.current.setAll(['a', 'b'], true));
  unmount();
  const second = renderHook(useFolderExpansion);
  expect([...second.result.current.expanded]).toEqual(['a', 'b']);
  act(() => second.result.current.setAll([], false));
  expect(second.result.current.expanded.size).toBe(0);
  expect(JSON.parse(localStorage.getItem('3mf-katalog-sidebar-expanded')!)).toEqual([]);
});

it.each(['bad json', '{}', '[1,"a",null]'])('tolerates invalid stored expansion: %s', (value) => {
  localStorage.setItem('3mf-katalog-sidebar-expanded', value);
  const { result } = renderHook(useFolderExpansion);
  expect([...result.current.expanded]).toEqual(value.startsWith('[') ? ['a'] : []);
});

it('still operates when storage is unavailable', () => {
  const get = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw Error('denied'); });
  const set = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw Error('full'); });
  try {
    const { result } = renderHook(useFolderExpansion);
    act(() => result.current.expand('a'));
    expect(result.current.isExpanded('a')).toBe(true);
  } finally { get.mockRestore(); set.mockRestore(); }
});
