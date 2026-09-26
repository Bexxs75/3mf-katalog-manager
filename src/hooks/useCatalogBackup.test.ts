import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, cleanup } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useCatalogBackup } from './useCatalogBackup';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  cleanup();
});

describe('useCatalogBackup', () => {
  it('exportCatalog reads the known settings keys and calls export_catalog', async () => {
    localStorage.setItem('3mf-katalog-theme', 'dark');
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.exportCatalog();
    });
    expect(invoke).toHaveBeenCalledWith('export_catalog', expect.any(Object));
    const call = vi.mocked(invoke).mock.calls[0];
    const settings = JSON.parse((call[1] as { settingsJson: string }).settingsJson);
    expect(settings['3mf-katalog-theme']).toBe('dark');
  });

  it('exportCatalog sets catalogBackupError on failure', async () => {
    vi.mocked(invoke).mockRejectedValue('disk full');
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.exportCatalog();
    });
    expect(result.current.catalogBackupError).toBe('disk full');
  });

  it('importCatalog restores settings and calls onImported when imported=true', async () => {
    vi.mocked(invoke).mockResolvedValue({
      imported: true,
      settingsJson: JSON.stringify({ '3mf-katalog-theme': 'light' }),
    });
    const onImported = vi.fn();
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.importCatalog(onImported);
    });
    expect(onImported).toHaveBeenCalled();
    expect(localStorage.getItem('3mf-katalog-theme')).toBe('light');
  });

  it('Finding 7 (Abschluss-Review): exportCatalog no longer exports the dead slicer localStorage key', async () => {
    // The slicer registry lives in the backend; a legacy value in localStorage is a
    // machine-local path and must not go into the backup.
    localStorage.setItem('3mf-katalog-slicers', JSON.stringify({ slicers: [{ id: 'x', path: '/opt/old-slicer' }] }));
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.exportCatalog();
    });
    const call = vi.mocked(invoke).mock.calls[0];
    const settings = JSON.parse((call[1] as { settingsJson: string }).settingsJson);
    expect(settings).not.toHaveProperty('3mf-katalog-slicers');
  });

  it('importCatalog never restores the slicer list from a backup (Finding Z-1)', async () => {
    // Slicer paths are launched as processes via open_in_slicer - a
    // foreign backup must not be able to store anything there.
    localStorage.setItem('3mf-katalog-slicers', 'eigene-liste');
    vi.mocked(invoke).mockResolvedValue({
      imported: true,
      settingsJson: JSON.stringify({
        '3mf-katalog-slicers': JSON.stringify({ slicers: [{ id: 'x', name: 'shell', path: '/bin/sh' }] }),
      }),
    });
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.importCatalog(vi.fn());
    });
    expect(localStorage.getItem('3mf-katalog-slicers')).toBe('eigene-liste');
  });

  it('importCatalog skips settings values outside the allowed set', async () => {
    localStorage.setItem('3mf-katalog-theme', 'dark');
    localStorage.setItem('3mf-katalog-language', 'de');
    vi.mocked(invoke).mockResolvedValue({
      imported: true,
      settingsJson: JSON.stringify({
        '3mf-katalog-theme': '<img src=x onerror=alert(1)>',
        '3mf-katalog-language': 'kl',
        '3mf-katalog-density': 'comfort',
      }),
    });
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.importCatalog(vi.fn());
    });
    expect(localStorage.getItem('3mf-katalog-theme')).toBe('dark');
    expect(localStorage.getItem('3mf-katalog-language')).toBe('de');
    // Valid values are still taken over normally.
    expect(localStorage.getItem('3mf-katalog-density')).toBe('comfort');
  });

  it('importCatalog does not call onImported when imported=false', async () => {
    vi.mocked(invoke).mockResolvedValue({ imported: false, settingsJson: null });
    const onImported = vi.fn();
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.importCatalog(onImported);
    });
    expect(onImported).not.toHaveBeenCalled();
  });

  it('importCatalog sets catalogBackupError on failure', async () => {
    vi.mocked(invoke).mockRejectedValue('bad zip');
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.importCatalog(vi.fn());
    });
    expect(result.current.catalogBackupError).toBe('bad zip');
  });
});
