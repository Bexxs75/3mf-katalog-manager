import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { showAfterPaint } from './startup';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
let frames: FrameRequestCallback[];
beforeEach(() => {
  frames = [];
  Object.defineProperty(window, '__TAURI_INTERNALS__', {value: {}, configurable: true});
  vi.stubGlobal('requestAnimationFrame', vi.fn(callback => { frames.push(callback); return frames.length; }));
  vi.stubGlobal('cancelAnimationFrame', vi.fn());
  vi.mocked(invoke).mockClear();
});
afterEach(() => { delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; vi.unstubAllGlobals(); });
it('waits for two frames before the native show signal', () => {
  showAfterPaint();
  expect(invoke).not.toHaveBeenCalled();
  frames.shift()!(0);
  expect(invoke).not.toHaveBeenCalled();
  frames.shift()!(1);
  expect(invoke).toHaveBeenCalledExactlyOnceWith('frontend_ready');
});
it('does not show an unmounted surface', () => {
  const cancel = showAfterPaint();
  cancel();
  frames.shift()!(0);
  frames.shift()!(1);
  expect(invoke).not.toHaveBeenCalled();
});
it('also waits for the local fonts before scheduling paint', async () => {
  let ready!: () => void;
  Object.defineProperty(document, 'fonts', {value: {ready: new Promise<void>(resolve => { ready = resolve; })}, configurable: true});
  try {
    showAfterPaint();
    expect(frames).toHaveLength(0);
    ready(); await Promise.resolve();
    expect(frames).toHaveLength(1);
    frames.shift()!(0); frames.shift()!(1);
    expect(invoke).toHaveBeenCalledExactlyOnceWith('frontend_ready');
  } finally { delete (document as unknown as Record<string, unknown>).fonts; }
});
