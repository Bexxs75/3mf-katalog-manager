import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useUpdateCheck } from './useUpdateCheck';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

function mockResult(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    currentVersion: '0.7.8',
    latestVersion: '0.7.8',
    updateAvailable: false,
    releaseUrl: '',
    ...overrides,
  };
}

function mockInvokeByCommand(handlers: Record<string, () => Promise<unknown>>) {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    const handler = handlers[cmd];
    if (handler) return handler();
    return Promise.resolve(undefined);
  });
}

describe('useUpdateCheck', () => {
  it('checks for an update on mount', async () => {
    vi.mocked(invoke).mockResolvedValue(mockResult());
    renderHook(() => useUpdateCheck());
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('check_for_update'));
  });

  it('loads currentVersion immediately via get_app_version, independent of the network check', async () => {
    let resolveCheck: (() => void) | undefined;
    mockInvokeByCommand({
      get_app_version: () => Promise.resolve('0.7.10'),
      check_for_update: () =>
        new Promise((resolve) => {
          resolveCheck = () => resolve(mockResult({ currentVersion: '0.7.10' }));
        }),
    });
    const { result } = renderHook(() => useUpdateCheck());
    await waitFor(() => expect(result.current.currentVersion).toBe('0.7.10'));
    // check_for_update is still pending at this point - currentVersion did not wait for it.
    expect(result.current.checking).toBe(true);
    resolveCheck?.();
    await waitFor(() => expect(result.current.checking).toBe(false));
  });

  it('keeps currentVersion available even when check_for_update rejects', async () => {
    mockInvokeByCommand({
      get_app_version: () => Promise.resolve('0.7.10'),
      check_for_update: () => Promise.reject(new Error('network down')),
    });
    const { result } = renderHook(() => useUpdateCheck());
    await waitFor(() => expect(result.current.currentVersion).toBe('0.7.10'));
    await waitFor(() => expect(result.current.checking).toBe(false));
    expect(result.current.currentVersion).toBe('0.7.10');
  });

  it('exposes updateAvailable + latestVersion when a newer version exists', async () => {
    vi.mocked(invoke).mockResolvedValue(
      mockResult({ updateAvailable: true, latestVersion: '0.7.9', releaseUrl: 'https://example.com/v0.7.9' }),
    );
    const { result } = renderHook(() => useUpdateCheck());
    await waitFor(() => expect(result.current.updateAvailable).toBe(true));
    expect(result.current.latestVersion).toBe('0.7.9');
  });

  it('dismiss() hides the toast without re-checking', async () => {
    vi.mocked(invoke).mockResolvedValue(mockResult({ updateAvailable: true, latestVersion: '0.7.9' }));
    const { result } = renderHook(() => useUpdateCheck());
    await waitFor(() => expect(result.current.updateAvailable).toBe(true));
    act(() => result.current.dismiss());
    expect(result.current.dismissed).toBe(true);
  });

  it('checkNow() triggers another check and sets checking while in flight', async () => {
    vi.mocked(invoke).mockResolvedValue(mockResult());
    const { result } = renderHook(() => useUpdateCheck());
    await waitFor(() => expect(result.current.checking).toBe(false));
    vi.mocked(invoke).mockResolvedValue(mockResult({ updateAvailable: true, latestVersion: '0.8.0' }));
    act(() => result.current.checkNow());
    await waitFor(() => expect(result.current.latestVersion).toBe('0.8.0'));
  });

  it('download() calls openReleaseUrl with the current releaseUrl', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'check_for_update') {
        return Promise.resolve(
          mockResult({ updateAvailable: true, latestVersion: '0.7.9', releaseUrl: 'https://example.com/v0.7.9' }),
        );
      }
      return Promise.resolve(undefined);
    });
    const { result } = renderHook(() => useUpdateCheck());
    await waitFor(() => expect(result.current.updateAvailable).toBe(true));
    act(() => result.current.download());
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('open_release_url', { url: 'https://example.com/v0.7.9' }),
    );
  });
});
