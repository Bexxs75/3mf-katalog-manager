import { uiSettings } from '../lib/uiSettings';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, cleanup } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useCatalogBackup } from './useCatalogBackup';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

function mockImported(result: { imported: boolean; settingsJson: string; values: Record<string, string>; settingsError?: { message: string; expected: boolean } }) {
  const { values, ...dto } = result;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'import_catalog') return dto;
    if (command === 'get_ui_settings') return { schema_version: 1, migrated_from_webview: true, values };
    throw new Error(`Unexpected command: ${command}`);
  });
}


beforeEach(() => {
  vi.stubGlobal('__TAURI_INTERNALS__', {});
  localStorage.clear();
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  vi.unstubAllGlobals();
  cleanup();
});

describe('useCatalogBackup', () => {
  it('exportCatalog leaves settings collection to the backend', async () => {
    uiSettings.setItem('3mf-katalog-theme', 'dark');
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.exportCatalog();
    });
    expect(invoke).toHaveBeenCalledWith('export_catalog');
  });

  it('exportCatalog sets catalogBackupError on failure', async () => {
    vi.mocked(invoke).mockRejectedValue('disk full');
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.exportCatalog();
    });
    expect(result.current.catalogBackupError).toEqual({ message: 'disk full', unexpected: true });
  });

  it('importCatalog restores settings and calls onImported when imported=true', async () => {
    mockImported({
      imported: true,
      settingsJson: JSON.stringify({ '3mf-katalog-theme': 'light' }),
      values: { '3mf-katalog-theme': 'light' },
    });
    const onImported = vi.fn();
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.importCatalog(onImported);
    });
    expect(onImported).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith('get_ui_settings', undefined);
    expect(uiSettings.getItem('3mf-katalog-theme')).toBe('light');
  });

  it.each(['slate', 'white'])('importCatalog restores the %s color scheme', async theme => {
    mockImported({
      imported: true,
      settingsJson: JSON.stringify({ '3mf-katalog-theme': theme }),
      values: { '3mf-katalog-theme': theme },
    });
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.importCatalog(vi.fn());
    });
    expect(uiSettings.getItem('3mf-katalog-theme')).toBe(theme);
  });

  it('Finding 7 (Abschluss-Review): exportCatalog no longer exports the dead slicer localStorage key', async () => {
    // The slicer registry lives in the backend; a legacy value in localStorage is a
    // machine-local path and must not go into the backup.
    uiSettings.setItem('3mf-katalog-slicers', JSON.stringify({ slicers: [{ id: 'x', path: '/opt/old-slicer' }] }));
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.exportCatalog();
    });
    expect(invoke).toHaveBeenCalledWith('export_catalog');
  });

  it('importCatalog never restores the slicer list from a backup (Finding Z-1)', async () => {
    // Slicer paths are launched as processes via open_in_slicer - a
    // foreign backup must not be able to store anything there.
    uiSettings.setItem('3mf-katalog-slicers', 'eigene-liste');
    mockImported({
      imported: true,
      values: { '3mf-katalog-slicers': 'eigene-liste' },
      settingsJson: JSON.stringify({
        '3mf-katalog-slicers': JSON.stringify({ slicers: [{ id: 'x', name: 'shell', path: '/bin/sh' }] }),
      }),
    });
    const { result } = renderHook(() => useCatalogBackup());
    await act(async () => {
      await result.current.importCatalog(vi.fn());
    });
    expect(uiSettings.getItem('3mf-katalog-slicers')).toBe('eigene-liste');
  });

  it('importCatalog uses the validated backend snapshot instead of the raw archive', async () => {
    uiSettings.setItem('3mf-katalog-theme', 'dark');
    uiSettings.setItem('3mf-katalog-language', 'de');
    mockImported({
      imported: true,
      values: { '3mf-katalog-theme': 'dark', '3mf-katalog-language': 'de', '3mf-katalog-density': 'comfort' },
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
    expect(uiSettings.getItem('3mf-katalog-theme')).toBe('dark');
    expect(uiSettings.getItem('3mf-katalog-language')).toBe('de');
    // Valid values are still taken over normally.
    expect(uiSettings.getItem('3mf-katalog-density')).toBe('comfort');
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
    expect(result.current.catalogBackupError).toEqual({ message: 'bad zip', unexpected: true });
  });
});

it('reports whether export actually saved a backup', async () => {
  const { result } = renderHook(() => useCatalogBackup());
  for (const saved of [false, true]) {
    vi.mocked(invoke).mockResolvedValueOnce(saved);
    await act(async () => expect(await result.current.exportCatalog()).toBe(saved));
  }
  vi.mocked(invoke).mockRejectedValueOnce('disk full');
  await act(async () => expect(await result.current.exportCatalog()).toBe(false));
});

it('clears the previous import error before confirmation and before choosing another file', async () => {
  const { result } = renderHook(() => useCatalogBackup());
  vi.mocked(invoke).mockRejectedValueOnce('bad zip');
  await act(() => result.current.importCatalog(vi.fn()));
  expect(result.current.catalogBackupError?.message).toBe('bad zip');
  act(() => result.current.clearCatalogBackupError());
  expect(result.current.catalogBackupError).toBeNull();
  vi.mocked(invoke).mockRejectedValueOnce('bad again');
  await act(() => result.current.importCatalog(vi.fn()));
  vi.mocked(invoke).mockResolvedValueOnce({ imported: false, settingsJson: null });
  await act(() => result.current.importCatalog(vi.fn()));
  expect(result.current.catalogBackupError).toBeNull();
});

it('neither exports nor restores the local last-seen release marker', async () => {
  const key = '3mf-katalog-last-seen-version';
  uiSettings.setItem(key, '0.18.3');
  vi.mocked(invoke).mockReset().mockResolvedValue(undefined);
  const { result } = renderHook(() => useCatalogBackup());
  await act(async () => { await result.current.exportCatalog(); });
  expect(invoke).toHaveBeenCalledWith('export_catalog');
  mockImported({ imported: true, settingsJson: JSON.stringify({ [key]: '0.18.0' }), values: { [key]: '0.18.3' } });
  await act(async () => { await result.current.importCatalog(vi.fn()); });
  expect(uiSettings.getItem(key)).toBe('0.18.3');
  mockImported({ imported: true, settingsJson: JSON.stringify({ [key]: '0.18.0' }), values: {} });
  await act(async () => { await result.current.importCatalog(vi.fn()); });
  expect(uiSettings.getItem(key)).toBeNull();
});

it('surfaces settings restore warnings while reloading the replaced catalog', async () => {
  mockImported({ imported: true, settingsJson: '{}', values: {}, settingsError: { message: 'uiSettingsUnavailable', expected: true } });
  const onImported = vi.fn();
  const { result } = renderHook(() => useCatalogBackup());
  await act(() => result.current.importCatalog(onImported));
  expect(invoke).toHaveBeenCalledWith('get_ui_settings', undefined);
  expect(result.current.catalogBackupError?.message).toBe('uiSettingsUnavailable');
  expect(onImported).toHaveBeenCalledOnce();
});
