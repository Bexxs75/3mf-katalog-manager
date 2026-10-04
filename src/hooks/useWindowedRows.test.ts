import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useWindowedRows } from './useWindowedRows';

it('calculates aligned rows, overscan and exact spacers after scroll and resize', () => {
  const container = document.createElement('div');
  Object.defineProperty(container, 'clientHeight', { value: 200, configurable: true });
  let resize: (() => void) | undefined;
  vi.stubGlobal('ResizeObserver', class { constructor(callback: () => void) { resize = callback; } observe() {} disconnect() {} });
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => { cb(0); return 1; });
  vi.stubGlobal('cancelAnimationFrame', vi.fn());
  const { result, rerender } = renderHook(({ columns }) => useWindowedRows({ containerRef: { current: container }, itemCount: 100, columns, rowHeight: 50 }), { initialProps: { columns: 4 } });
  expect(result.current).toEqual({ startIndex: 0, endIndex: 24, topSpacer: 0, bottomSpacer: 950 });
  act(() => { container.scrollTop = 500; container.dispatchEvent(new Event('scroll')); });
  expect(result.current).toEqual({ startIndex: 32, endIndex: 64, topSpacer: 400, bottomSpacer: 450 });
  act(() => { Object.defineProperty(container, 'clientHeight', { value: 100 }); resize?.(); });
  expect(result.current.endIndex).toBe(56);
  rerender({ columns: 2 });
  expect(result.current.startIndex).toBe(16);
  vi.unstubAllGlobals();
});
it('accounts for a group offset and clamps an offscreen group', () => {
  const el = document.createElement('div');
  Object.defineProperty(el, 'clientHeight', { value: 200 });
  const { result } = renderHook(() => useWindowedRows({ containerRef: { current: el }, itemCount: 61, columns: 3, rowHeight: 50, offsetTop: 1000, overscan: 0 }));
  expect(result.current.endIndex).toBe(0);
  expect(result.current.bottomSpacer).toBe(1050);
});
it('clamps empty input, invalid columns and scroll beyond the final row', () => {
  const el = document.createElement('div');
  el.scrollTop = 100000;
  Object.defineProperty(el, 'clientHeight', { value: 300 });
  const { result, rerender } = renderHook(({ itemCount }) => useWindowedRows({ containerRef: { current: el }, itemCount, columns: 0, rowHeight: 40 }), { initialProps: { itemCount: 11 } });
  expect(result.current).toEqual({ startIndex: 11, endIndex: 11, topSpacer: 440, bottomSpacer: 0 });
  rerender({ itemCount: 0 });
  expect(result.current).toEqual({ startIndex: 0, endIndex: 0, topSpacer: 0, bottomSpacer: 0 });
});
