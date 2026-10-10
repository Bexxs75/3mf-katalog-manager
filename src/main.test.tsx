import { act } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('./App', () => ({ default: () => <div>App surface</div> }));
vi.mock('./i18n/LanguageContext', () => ({ LanguageProvider: ({ children }: { children: ReactNode }) => children }));
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
    ? new Promise((res, rej) => { resolve = res; reject = rej; }) : Promise.resolve());
  document.body.innerHTML = '<div id="root"></div>';
  await act(async () => { await import('./main'); });
  const paint = () => { const pending = [...frames.values()]; frames.clear(); pending.forEach(frame => frame(0)); };
  act(() => { paint(); paint(); });
  expect(invoke).not.toHaveBeenCalledWith('frontend_ready');
  expect(document.body).not.toHaveTextContent('App surface');
  await act(async () => { if (fail) reject(new Error('IPC failed')); else resolve({ container: true }); });
  expect(document.body).toHaveTextContent('App surface');
  act(() => { paint(); paint(); });
  expect(invoke).toHaveBeenCalledWith('frontend_ready');
});
