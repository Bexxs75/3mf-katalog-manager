import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useRegisterCatalogBaseDirOnStartup } from './useCatalogBaseDir';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('useRegisterCatalogBaseDirOnStartup', () => {
  it('registers an existing base dir once at startup and refreshes the folders', async () => {
    vi.mocked(invoke).mockResolvedValue({ id: '1', path: '/katalog' });
    const refreshFolders = vi.fn();
    const { rerender } = renderHook(({ dir }) => useRegisterCatalogBaseDirOnStartup(dir, refreshFolders), {
      initialProps: { dir: '/katalog' as string | null },
    });
    await waitFor(() => expect(refreshFolders).toHaveBeenCalledTimes(1));
    expect(invoke).toHaveBeenCalledWith('register_existing_catalog_base_dir', { path: '/katalog' });

    rerender({ dir: '/anders' });
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('does nothing without a base dir and ignores a missing directory', async () => {
    const refreshFolders = vi.fn();
    renderHook(() => useRegisterCatalogBaseDirOnStartup(null, refreshFolders));
    expect(invoke).not.toHaveBeenCalled();

    vi.mocked(invoke).mockResolvedValue(null);
    renderHook(() => useRegisterCatalogBaseDirOnStartup('/weg', refreshFolders));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('register_existing_catalog_base_dir', { path: '/weg' }));
    expect(invoke).not.toHaveBeenCalledWith('register_catalog_base_dir', expect.anything());
    expect(refreshFolders).not.toHaveBeenCalled();
  });

  it('logs instead of throwing when the backend call fails', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(invoke).mockRejectedValue('kaputt');
    renderHook(() => useRegisterCatalogBaseDirOnStartup('/katalog', vi.fn()));
    await waitFor(() => expect(error).toHaveBeenCalled());
    error.mockRestore();
  });
});
