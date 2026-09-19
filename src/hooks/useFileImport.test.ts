import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useFileImport } from './useFileImport';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => {}) }),
}));

beforeEach(() => { vi.mocked(invoke).mockReset(); });

function setup(overrides: Partial<Parameters<typeof useFileImport>[0]> = {}) {
  const onImported = vi.fn();
  const refreshFolders = vi.fn();
  const refreshFiles = vi.fn();
  const hook = renderHook(() =>
    useFileImport({
      enabled: true,
      catalogBaseDir: null,
      activeFolderId: 'all',
      onImported,
      refreshFolders,
      refreshFiles,
      ...overrides,
    }),
  );
  return { ...hook, onImported, refreshFolders, refreshFiles };
}

describe('useFileImport', () => {
  it('importFiles calls onImported and shows a banner when there are duplicates', async () => {
    vi.mocked(invoke).mockResolvedValue({ imported: [{ id: 'm1' }], duplicateCount: 2 });
    const { result, onImported } = setup();
    await act(async () => result.current.importFiles());
    expect(invoke).toHaveBeenCalledWith('import_files');
    expect(onImported).toHaveBeenCalledWith({ imported: [{ id: 'm1' }], duplicateCount: 2 });
    expect(result.current.importBanner).toEqual({ imported: 1, duplicates: 2 });
  });

  it('importFiles does not show a banner when there are no duplicates', async () => {
    vi.mocked(invoke).mockResolvedValue({ imported: [{ id: 'm1' }], duplicateCount: 0 });
    const { result } = setup();
    await act(async () => result.current.importFiles());
    expect(result.current.importBanner).toBeNull();
  });

  it('importFiles auto-files into the base-dir folder when catalogBaseDir is set and no folder is active', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'import_files') return Promise.resolve({ imported: [{ id: 'm1' }], duplicateCount: 0 });
      if (cmd === 'list_folders') return Promise.resolve([{ id: 'f1', path: '/base', parentId: null }]);
      return Promise.resolve(undefined);
    });
    const { result, refreshFolders, refreshFiles } = setup({ catalogBaseDir: '/base' });
    await act(async () => result.current.importFiles());
    expect(invoke).toHaveBeenCalledWith('move_file_to_folder', { fileId: 'm1', folderId: 'f1' });
    expect(refreshFolders).toHaveBeenCalled();
    expect(refreshFiles).toHaveBeenCalled();
  });

  it('dismissImportBanner clears the banner', async () => {
    vi.mocked(invoke).mockResolvedValue({ imported: [{ id: 'm1' }], duplicateCount: 1 });
    const { result } = setup();
    await act(async () => result.current.importFiles());
    act(() => result.current.dismissImportBanner());
    expect(result.current.importBanner).toBeNull();
  });
});
