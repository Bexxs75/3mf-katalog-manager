import { act, renderHook } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { useDetailPanel } from './useDetailPanel';
beforeEach(() => { localStorage.clear(); vi.restoreAllMocks(); });
it('defaults to auto and persists and reads pinned', () => {
  const { result, unmount } = renderHook(useDetailPanel);
  expect(result.current.detailPanel).toBe('auto');
  act(() => result.current.setDetailPanel('pinned'));
  expect(localStorage.getItem('3mf-katalog-detail-panel')).toBe('pinned');
  unmount();
  expect(renderHook(useDetailPanel).result.current.detailPanel).toBe('pinned');
});
it('works when storage is unavailable', () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('denied'); });
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('denied'); });
  const { result } = renderHook(useDetailPanel);
  expect(result.current.detailPanel).toBe('auto');
  act(() => result.current.setDetailPanel('pinned'));
  expect(result.current.detailPanel).toBe('pinned');
});
