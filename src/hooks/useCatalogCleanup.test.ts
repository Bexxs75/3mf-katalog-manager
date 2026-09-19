import { describe, expect, it, vi, afterEach } from 'vitest';
import { renderHook, act, cleanup } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useCatalogCleanup } from './useCatalogCleanup';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

afterEach(() => {
  cleanup();
});

describe('useCatalogCleanup', () => {
  it('scanCatalogIssues opens the dialog with the returned issues', async () => {
    vi.mocked(invoke).mockResolvedValue({ orphaned: [], duplicateGroups: [] });
    const { result } = renderHook(() => useCatalogCleanup());
    await act(async () => {
      await result.current.scanCatalogIssues();
    });
    expect(result.current.cleanupDialogOpen).toBe(true);
    expect(result.current.cleanupScanning).toBe(false);
    expect(invoke).toHaveBeenCalledWith('scan_catalog_issues');
  });

  it('scanCatalogIssues sets cleanupError on failure', async () => {
    vi.mocked(invoke).mockRejectedValue('scan failed');
    const { result } = renderHook(() => useCatalogCleanup());
    await act(async () => {
      await result.current.scanCatalogIssues();
    });
    expect(result.current.cleanupError).toBe('scan failed');
    expect(result.current.cleanupDialogOpen).toBe(false);
  });

  it('deleteSelectedCleanupFiles closes the dialog and calls onDeleted', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const onDeleted = vi.fn();
    const { result } = renderHook(() => useCatalogCleanup());
    await act(async () => {
      await result.current.deleteSelectedCleanupFiles(['m1'], onDeleted);
    });
    expect(invoke).toHaveBeenCalledWith('delete_files', { fileIds: ['m1'] });
    expect(onDeleted).toHaveBeenCalledWith(['m1']);
    expect(result.current.cleanupDialogOpen).toBe(false);
    expect(result.current.cleanupIssues).toBeNull();
  });

  it('deleteSelectedCleanupFiles sets cleanupError on failure and keeps dialog open', async () => {
    vi.mocked(invoke).mockRejectedValue('delete failed');
    const onDeleted = vi.fn();
    const { result } = renderHook(() => useCatalogCleanup());
    await act(async () => {
      await result.current.deleteSelectedCleanupFiles(['m1'], onDeleted);
    });
    expect(onDeleted).not.toHaveBeenCalled();
    expect(result.current.cleanupError).toBe('delete failed');
  });
});
