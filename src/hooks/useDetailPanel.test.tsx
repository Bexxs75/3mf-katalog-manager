import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useDetailPanel } from './useDetailPanel';

beforeEach(() => localStorage.clear());
afterEach(() => { vi.unstubAllEnvs(); localStorage.clear(); });

it('keeps the detail panel automatic by default in the desktop build and honors a stored choice', () => {
  vi.stubEnv('VITE_TARGET', 'desktop');
  expect(renderHook(() => useDetailPanel()).result.current.detailPanel).toBe('auto');
  localStorage.setItem('3mf-katalog-detail-panel', 'pinned');
  expect(renderHook(() => useDetailPanel()).result.current.detailPanel).toBe('pinned');
});

it('always pins the detail panel in the web build, whatever an older build stored', () => {
  vi.stubEnv('VITE_TARGET', 'web');
  localStorage.setItem('3mf-katalog-detail-panel', 'auto');
  const { result } = renderHook(() => useDetailPanel());
  expect(result.current.detailPanel).toBe('pinned');
  act(() => result.current.setDetailPanel('auto'));
  expect(result.current.detailPanel).toBe('pinned');
});
