import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useSidebarWidth } from './useSidebarWidth';

const key = '3mf-katalog-sidebar-width';
beforeEach(() => { localStorage.clear(); vi.stubGlobal('innerWidth', 1200); });
afterEach(() => vi.unstubAllGlobals());

it('defaults to 242, clamps requests and resets', () => {
  const { result } = renderHook(useSidebarWidth);
  expect(result.current.width).toBe(242);
  act(() => result.current.setWidth(900));
  expect(result.current.width).toBe(420);
  act(() => result.current.setWidth(10));
  expect(result.current.width).toBe(180);
  act(() => result.current.reset());
  expect(result.current.width).toBe(242);
  expect(localStorage.getItem(key)).toBe('242');
});

it('keeps the requested width across window resizing and remounting', () => {
  const { result, unmount } = renderHook(useSidebarWidth);
  act(() => result.current.setWidth(420));
  act(() => { vi.stubGlobal('innerWidth', 800); window.dispatchEvent(new Event('resize')); });
  expect(result.current.width).toBe(260);
  expect(localStorage.getItem(key)).toBe('420');
  act(() => { vi.stubGlobal('innerWidth', 600); window.dispatchEvent(new Event('resize')); });
  expect(result.current.width).toBe(180);
  unmount();
  vi.stubGlobal('innerWidth', 1200);
  expect(renderHook(useSidebarWidth).result.current.width).toBe(420);
});

it.each(['NaN', 'Infinity', 'null', '{}', ''])('uses default for malformed storage: %s', (value) => {
  localStorage.setItem(key, value);
  expect(renderHook(useSidebarWidth).result.current.width).toBe(242);
});

it('ignores nonfinite requests and survives storage errors', () => {
  const get = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw Error('denied'); });
  const set = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw Error('full'); });
  try {
    const { result } = renderHook(useSidebarWidth);
    act(() => result.current.setWidth(NaN));
    expect(result.current.width).toBe(242);
    act(() => result.current.setWidth(300));
    expect(result.current.width).toBe(300);
  } finally { get.mockRestore(); set.mockRestore(); }
});
