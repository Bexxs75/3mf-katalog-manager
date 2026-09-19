import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor, fireEvent } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useFolderDragAndDrop } from './useFolderDragAndDrop';
import { makeModelFile, makeFolder } from '../test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('useFolderDragAndDrop', () => {
  it('onCreateFolder calls create_folder and refreshes folders on success', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const refreshFolders = vi.fn();
    const refreshFiles = vi.fn();
    const { result } = renderHook(() =>
      useFolderDragAndDrop([], [], { refreshFolders, refreshFiles }),
    );
    await act(async () => result.current.onCreateFolder(null, 'New'));
    expect(invoke).toHaveBeenCalledWith('create_folder', { parentId: null, name: 'New' });
    await waitFor(() => expect(refreshFolders).toHaveBeenCalled());
  });

  it('dragging a file onto a folder and releasing the mouse moves it', async () => {
    const folders = [makeFolder({ id: 'f1', path: '/Target' })];
    const models = [makeModelFile({ id: 'm1', name: 'Model' })];
    vi.mocked(invoke).mockResolvedValue(undefined);
    const refreshFolders = vi.fn();
    const refreshFiles = vi.fn();
    const { result } = renderHook(() =>
      useFolderDragAndDrop(models, folders, { refreshFolders, refreshFiles }),
    );
    act(() => result.current.onDragFileStart('m1'));
    act(() => result.current.handleFolderMouseEnter('f1'));
    expect(result.current.dragOverFolderId).toBe('f1');
    await act(async () => {
      fireEvent.mouseUp(document);
    });
    expect(invoke).toHaveBeenCalledWith('move_file_to_folder', { fileId: 'm1', folderId: 'f1' });
    await waitFor(() => expect(result.current.moveToast).toEqual({ from: 'Model', to: '/Target' }));
    expect(result.current.draggedFileId).toBeNull();
  });

  it('handleFolderMouseEnter refuses a folder-drag onto its own descendant', () => {
    const folders = [
      makeFolder({ id: 'parent', parentId: null }),
      makeFolder({ id: 'child', parentId: 'parent' }),
    ];
    const { result } = renderHook(() => useFolderDragAndDrop([], folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }));
    act(() => result.current.onDragFolderStart('parent'));
    act(() => result.current.handleFolderMouseEnter('child'));
    expect(result.current.dragOverFolderId).toBeNull();
  });
});
