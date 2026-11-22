import { act } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onCloseRequested: vi.fn().mockResolvedValue(vi.fn()), destroy: vi.fn().mockResolvedValue(undefined) }) }));
vi.mock('./App', () => ({ default: () => <div>App surface</div> }));
vi.mock('./i18n/LanguageContext', () => ({ LanguageProvider: ({ children }: { children: ReactNode }) => children, useT: () => (key: string) => key }));
vi.mock('./hooks/UiDensityContext', () => ({ UiDensityProvider: ({ children }: { children: ReactNode }) => children }));
vi.mock('./hooks/ModelLayoutContext', () => ({ ModelLayoutProvider: ({ children }: { children: ReactNode }) => children }));
vi.mock('./diagnostics/DiagnosticsContext', () => ({ DiagnosticsProvider: ({ children }: { children: ReactNode }) => children }));
vi.mock('./diagnostics/ErrorBoundary', () => ({ ErrorBoundary: ({ children }: { children: ReactNode }) => children, CrashFallback: () => null }));
vi.mock('./lib/contextMenuGuard', () => ({ installContextMenuGuard: vi.fn() }));
afterEach(() => { vi.unstubAllGlobals(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; document.body.innerHTML = ''; });

it.each([false, true])('shows the native window only after runtime resolution, including error fallback (%s)', async fail => {
  vi.resetModules();
  const frames = new Map<number, FrameRequestCallback>();
  let nextFrame = 0;
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { frames.set(++nextFrame, callback); return nextFrame; });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id));
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
  let resolve!: (value: { container: boolean }) => void;
  let reject!: (error: Error) => void;
  vi.mocked(invoke).mockReset().mockImplementation(command => command === 'get_runtime_environment'
    ? new Promise((res, rej) => { resolve = res; reject = rej; })
    : command === 'get_ui_settings'
      ? fail ? Promise.reject(new Error('Settings unavailable')) : Promise.resolve({ schema_version: 1, migrated_from_webview: true, values: {} })
      : Promise.resolve());
  document.body.innerHTML = '<div id="root"></div>';
  await act(async () => { await import('./main'); });
  const paint = () => { const pending = [...frames.values()]; frames.clear(); pending.forEach(frame => frame(0)); };
  act(() => { paint(); paint(); });
  expect(invoke).not.toHaveBeenCalledWith('frontend_ready');
  expect(document.body).not.toHaveTextContent('App surface');
  await act(async () => { if (fail) reject(new Error('IPC failed')); else resolve({ container: true }); });
  expect(document.body).toHaveTextContent('App surface');
  if (fail) expect(document.querySelector('[role="alert"]')).toHaveTextContent('errUiSettingsUnavailable');
  else expect(document.querySelector('[role="alert"]')).toBeNull();
  act(() => { paint(); paint(); });
  expect(invoke).toHaveBeenCalledWith('frontend_ready');
});

it.each(['get_ui_settings', 'migrate_ui_settings'])('mounts and calls frontend_ready after %s times out', async stalled => {
  vi.resetModules();
  vi.useFakeTimers();
  try {
    const frames = new Map<number, FrameRequestCallback>();
    let frame = 0;
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { frames.set(++frame, callback); return frame; });
    vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id));
    vi.stubGlobal('__TAURI_INTERNALS__', {});
    localStorage.setItem('3mf-katalog-theme', 'dark');
    vi.mocked(invoke).mockReset().mockImplementation(command => {
      if (command === stalled) return new Promise(() => {});
      if (command === 'get_runtime_environment') return Promise.resolve({ container: true });
      if (command === 'get_ui_settings') return Promise.resolve({ schema_version: 1, migrated_from_webview: false, values: {} });
      return Promise.resolve();
    });
    document.body.innerHTML = '<div id="root"></div>';
    await act(async () => { await import('./main'); });
    expect(document.body).not.toHaveTextContent('App surface');
    await act(async () => { await vi.advanceTimersByTimeAsync(3000); });
    expect(document.body).toHaveTextContent('App surface');
    expect(document.querySelector('[role="alert"]')).toHaveTextContent('errUiSettingsUnavailable');
    const paint = () => { const callbacks = [...frames.values()]; frames.clear(); callbacks.forEach(callback => callback(0)); };
    act(() => { paint(); paint(); });
    expect(invoke).toHaveBeenCalledWith('frontend_ready');
    expect(localStorage.getItem('3mf-katalog-theme')).toBe('dark');
  } finally { vi.useRealTimers(); }
});
