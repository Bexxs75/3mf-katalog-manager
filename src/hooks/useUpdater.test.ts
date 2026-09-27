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
  getAppVersion: vi.fn(),
  openReleaseUrl: vi.fn(),
  isPreviewBuild: vi.fn(),
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
  vi.mocked(updateApi.getAppVersion).mockReset().mockResolvedValue('0.15.0');
  vi.mocked(updateApi.openReleaseUrl).mockReset().mockResolvedValue(undefined);
  vi.mocked(updateApi.isPreviewBuild).mockReset().mockResolvedValue(false);
});

describe('useUpdater', () => {
  it('checks for an update on mount and stores the result', async () => {
    vi.mocked(api.checkAppUpdate).mockResolvedValue(mockInfo({ availableVersion: '0.15.1' }));
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.info?.availableVersion).toBe('0.15.1'));
    expect(result.current.phase).toBe('idle');
  });

  it('loads currentVersion immediately via getAppVersion, independent of the update check', async () => {
    let resolveCheck!: (v: api.UpdateInfo) => void;
    vi.mocked(api.checkAppUpdate).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveCheck = resolve;
        }),
    );
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.currentVersion).toBe('0.15.0'));
    // The check is still pending at this point - currentVersion did not wait for it.
    expect(result.current.phase).toBe('checking');
    resolveCheck(mockInfo());
    await waitFor(() => expect(result.current.phase).toBe('idle'));
  });

  it('keeps currentVersion available even when the update check rejects', async () => {
    vi.mocked(api.checkAppUpdate).mockRejectedValue(new Error('network down'));
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.currentVersion).toBe('0.15.0'));
    await waitFor(() => expect(result.current.phase).toBe('idle'));
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

  it('startUpdate adopts the version actually downloaded, e.g. when it differs from the last check', async () => {
    vi.mocked(api.checkAppUpdate).mockResolvedValue(mockInfo({ availableVersion: '0.15.1' }));
    vi.mocked(api.downloadAppUpdate).mockResolvedValue('0.15.2');
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.info?.availableVersion).toBe('0.15.1'));

    act(() => result.current.startUpdate());
    await waitFor(() => expect(result.current.phase).toBe('ready'));
    expect(result.current.info?.availableVersion).toBe('0.15.2');
    expect(result.current.info?.releaseUrl).toBe(
      'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.15.2',
    );
  });

  it('a rejected download sets phase "error" with an AppError', async () => {
    vi.mocked(api.downloadAppUpdate).mockRejectedValue({ message: 'Download fehlgeschlagen', expected: false });
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => result.current.startUpdate());
    await waitFor(() => expect(result.current.phase).toBe('error'));
    expect(result.current.error).toEqual({ message: 'Download fehlgeschlagen', unexpected: true });
  });

  it('a rejected install (unexpected, e.g. a failed backup) sets phase "error" with an AppError', async () => {
    vi.mocked(api.installAppUpdate).mockRejectedValue({ message: 'Sicherung fehlgeschlagen', expected: false });
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => result.current.install());
    await waitFor(() => expect(result.current.phase).toBe('error'));
    expect(result.current.error).toEqual({ message: 'Sicherung fehlgeschlagen', unexpected: true });
  });

  it('retry after a failed install calls installAppUpdate again, not downloadAppUpdate', async () => {
    vi.mocked(api.installAppUpdate)
      .mockRejectedValueOnce({ message: 'Sicherung fehlgeschlagen', expected: false })
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

  it('an install failure that leaves no pending update falls back to a fresh download instead of looping', async () => {
    // First install: an unexpected failure (e.g. the backup failed) - the
    // backend keeps the pending update, so retry() calls install() again.
    // Second install: the backend has no pending update left at all (an
    // expected error) - retrying install a third time could only ever repeat
    // this, so the hook must fall back to downloading instead of looping.
    vi.mocked(api.installAppUpdate)
      .mockRejectedValueOnce({ message: 'Sicherung fehlgeschlagen', expected: false })
      .mockRejectedValueOnce({ message: 'Es liegt kein heruntergeladenes Update vor', expected: true });
    vi.mocked(api.downloadAppUpdate).mockResolvedValue('0.15.1');
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => result.current.install());
    await waitFor(() => expect(result.current.phase).toBe('error'));

    act(() => result.current.retry());
    await waitFor(() => expect(result.current.phase).toBe('ready'));
    expect(api.installAppUpdate).toHaveBeenCalledTimes(2);
    expect(api.downloadAppUpdate).toHaveBeenCalledTimes(1);
  });

  it('checkNow is a no-op while downloading, ready or installing', async () => {
    vi.mocked(api.downloadAppUpdate).mockImplementation(() => new Promise(() => {}));
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));
    expect(api.checkAppUpdate).toHaveBeenCalledTimes(1);

    act(() => result.current.startUpdate());
    expect(result.current.phase).toBe('downloading');

    act(() => result.current.checkNow());
    expect(api.checkAppUpdate).toHaveBeenCalledTimes(1);
    expect(result.current.phase).toBe('downloading');
  });

  it('startUpdate ignores a second call while a download is already in flight', async () => {
    vi.mocked(api.downloadAppUpdate).mockImplementation(() => new Promise(() => {}));
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => {
      result.current.startUpdate();
      result.current.startUpdate();
    });
    expect(api.downloadAppUpdate).toHaveBeenCalledTimes(1);
  });

  it('install ignores a second call while an install is already in flight', async () => {
    vi.mocked(api.installAppUpdate).mockImplementation(() => new Promise(() => {}));
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.phase).toBe('idle'));

    act(() => {
      result.current.install();
      result.current.install();
    });
    expect(api.installAppUpdate).toHaveBeenCalledTimes(1);
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

  it('preview is false by default and true once isPreviewBuild resolves true', async () => {
    vi.mocked(updateApi.isPreviewBuild).mockResolvedValue(true);
    const { result } = renderHook(() => useUpdater());
    expect(result.current.preview).toBe(false);
    await waitFor(() => expect(result.current.preview).toBe(true));
  });

  it('on a preview build, a download release URL points at the shared preview release page', async () => {
    vi.mocked(updateApi.isPreviewBuild).mockResolvedValue(true);
    vi.mocked(api.downloadAppUpdate).mockResolvedValue('0.15.0-2');
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.preview).toBe(true));

    act(() => result.current.startUpdate());
    await waitFor(() => expect(result.current.phase).toBe('ready'));
    expect(result.current.info?.releaseUrl).toBe(
      'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/preview',
    );
  });

  it('a failed openNotes is surfaced via notesError, not the general error phase', async () => {
    vi.mocked(api.checkAppUpdate).mockResolvedValue(
      mockInfo({ availableVersion: '0.15.1', canInstall: false, releaseUrl: 'https://example.com/v0.15.1' }),
    );
    vi.mocked(updateApi.openReleaseUrl).mockRejectedValue(new Error('no browser available'));
    const { result } = renderHook(() => useUpdater());
    await waitFor(() => expect(result.current.info?.availableVersion).toBe('0.15.1'));

    act(() => result.current.openNotes());
    await waitFor(() => expect(result.current.notesError).not.toBeNull());
    expect(result.current.notesError).toEqual({ message: 'no browser available', unexpected: true });
    expect(result.current.phase).not.toBe('error');
  });
});
