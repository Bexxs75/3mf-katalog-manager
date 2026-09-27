import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useUpdater } from './useUpdater';
import * as api from '../lib/api/updater';
import * as updateApi from '../lib/api/update';

vi.mock('../lib/api/updater', () => ({
  checkAppUpdate: vi.fn(),
  downloadAppUpdate: vi.fn(),
  discardAppUpdate: vi.fn(),
  installAppUpdate: vi.fn(),
}));
vi.mock('../lib/api/update', () => ({
  openReleaseUrl: vi.fn(),
}));

function mockInfo(overrides: Partial<api.UpdateInfo> = {}): api.UpdateInfo {
  return {
    currentVersion: '0.15.0',
    availableVersion: null,
    releaseUrl: null,
    canInstall: true,
    lastUpdate: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(api.checkAppUpdate).mockReset().mockResolvedValue(mockInfo());
  vi.mocked(api.downloadAppUpdate).mockReset();
  vi.mocked(api.discardAppUpdate).mockReset().mockResolvedValue(undefined);
  vi.mocked(api.installAppUpdate).mockReset();
  vi.mocked(updateApi.openReleaseUrl).mockReset().mockResolvedValue(undefined);
});

describe('useUpdater', () => {
  it('checks for an update on mount and stores the result', async () => {
    vi.mocked(api.checkAppUpdate).mockResolvedValue(mockInfo({ availableVersion: '0.15.1' }));
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.info?.availableVersion).toBe('0.15.1'));
    expect(result.current.phase).toBe('idle');
  });

  it('startUpdate reports progress via the callback and reaches ready on success', async () => {
    let onProgress!: (p: api.DownloadProgress) => void;
    let resolveDownload!: (v: string) => void;
    vi.mocked(api.downloadAppUpdate).mockImplementation((cb) => {
      onProgress = cb;
      return new Promise((resolve) => {
        resolveDownload = resolve;
      });
    });
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => result.current.startUpdate());
    expect(result.current.phase).toBe('downloading');

    act(() => onProgress({ downloaded: 10, total: 100 }));
    expect(result.current.progress).toEqual({ downloaded: 10, total: 100 });

    await act(async () => {
      resolveDownload('0.15.1');
      await Promise.resolve();
    });
    await waitFor(() => expect(result.current.phase).toBe('ready'));
  });

  it('a rejected download sets phase "error" with an AppError', async () => {
    vi.mocked(api.downloadAppUpdate).mockRejectedValue({ message: 'Download fehlgeschlagen', expected: false });
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => result.current.startUpdate());
    await waitFor(() => expect(result.current.phase).toBe('error'));
    expect(result.current.error).toEqual({ message: 'Download fehlgeschlagen', unexpected: true });
  });

  it('a rejected install sets phase "error" with an AppError', async () => {
    vi.mocked(api.installAppUpdate).mockRejectedValue({ message: 'Sicherung fehlgeschlagen', expected: true });
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => result.current.install());
    await waitFor(() => expect(result.current.phase).toBe('error'));
    expect(result.current.error).toEqual({ message: 'Sicherung fehlgeschlagen', unexpected: false });
  });

  it('retry after a failed install calls installAppUpdate again, not downloadAppUpdate', async () => {
    vi.mocked(api.installAppUpdate)
      .mockRejectedValueOnce({ message: 'Sicherung fehlgeschlagen', expected: true })
      .mockResolvedValueOnce(undefined);
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => result.current.install());
    await waitFor(() => expect(result.current.phase).toBe('error'));
    expect(api.installAppUpdate).toHaveBeenCalledTimes(1);

    act(() => result.current.retry());
    await waitFor(() => expect(api.installAppUpdate).toHaveBeenCalledTimes(2));
    expect(api.downloadAppUpdate).not.toHaveBeenCalled();
  });

  it('retry after a failed download calls downloadAppUpdate again', async () => {
    vi.mocked(api.downloadAppUpdate)
      .mockRejectedValueOnce({ message: 'Download fehlgeschlagen', expected: false })
      .mockResolvedValueOnce('0.15.1');
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => result.current.startUpdate());
    await waitFor(() => expect(result.current.phase).toBe('error'));
    expect(api.downloadAppUpdate).toHaveBeenCalledTimes(1);

    act(() => result.current.retry());
    await waitFor(() => expect(result.current.phase).toBe('ready'));
    expect(api.downloadAppUpdate).toHaveBeenCalledTimes(2);
  });

  it('later() discards the pending update and dismisses', async () => {
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => result.current.later());
    expect(api.discardAppUpdate).toHaveBeenCalled();
    expect(result.current.dismissed).toBe(true);
    expect(result.current.phase).toBe('idle');
  });

  it('openNotes() opens the release URL from info', async () => {
    vi.mocked(api.checkAppUpdate).mockResolvedValue(
      mockInfo({ availableVersion: '0.15.1', releaseUrl: 'https://example.com/v0.15.1' }),
    );
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.info?.releaseUrl).toBe('https://example.com/v0.15.1'));

    act(() => result.current.openNotes());
    expect(updateApi.openReleaseUrl).toHaveBeenCalledWith('https://example.com/v0.15.1');
  });
});
