import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useDetailPanel } from './useDetailPanel';

beforeEach(() => localStorage.clear());
afterEach(() => { vi.unstubAllEnvs(); localStorage.clear(); });

it('keeps the detail panel automatic by default in the desktop build', () => {
  vi.stubEnv('VITE_TARGET', 'desktop');
  expect(renderHook(() => useDetailPanel()).result.current.detailPanel).toBe('auto');
});

it('pins the detail panel by default in the web build so the grid does not jump', () => {
  vi.stubEnv('VITE_TARGET', 'web');
  expect(renderHook(() => useDetailPanel()).result.current.detailPanel).toBe('pinned');
});

it('respects an explicit choice in both builds, including a stored automatic mode', () => {
  vi.stubEnv('VITE_TARGET', 'web');
  const first = renderHook(() => useDetailPanel());
  act(() => first.result.current.setDetailPanel('auto'));
  expect(renderHook(() => useDetailPanel()).result.current.detailPanel).toBe('auto');
  vi.stubEnv('VITE_TARGET', 'desktop');
  localStorage.setItem('3mf-katalog-detail-panel', 'pinned');
  expect(renderHook(() => useDetailPanel()).result.current.detailPanel).toBe('pinned');
});

it('ignores an automatic mode stored by older web builds', () => {
  vi.stubEnv('VITE_TARGET', 'web');
  localStorage.setItem('3mf-katalog-detail-panel', 'auto');
  expect(renderHook(() => useDetailPanel()).result.current.detailPanel).toBe('pinned');
});
