import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor, fireEvent } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useFolderDragAndDrop } from './useFolderDragAndDrop';
import { makeModelFile, makeFolder } from '../test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); localStorage.clear(); });

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
    expect(localStorage.getItem('3mf-katalog-dnd-tip-dismissed')).toBe('true');
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

  it('handleFolderMouseLeave resets dragOverFolderId when leaving the current target', () => {
    const folders = [makeFolder({ id: 'f1' })];
    const { result } = renderHook(() => useFolderDragAndDrop([], folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }));
    act(() => result.current.onDragFileStart('m1'));
    act(() => result.current.handleFolderMouseEnter('f1'));
    expect(result.current.dragOverFolderId).toBe('f1');
    act(() => result.current.handleFolderMouseLeave('f1'));
    expect(result.current.dragOverFolderId).toBeNull();
  });

  it('handleFolderMouseLeave does not clobber a newer target from a stale event', () => {
    const folders = [makeFolder({ id: 'f1' }), makeFolder({ id: 'f2' })];
    const { result } = renderHook(() => useFolderDragAndDrop([], folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }));
    act(() => result.current.onDragFileStart('m1'));
    act(() => result.current.handleFolderMouseEnter('f1'));
    act(() => result.current.handleFolderMouseEnter('f2'));
    // Stale mouseleave for f1 arrives after the pointer already entered f2.
    act(() => result.current.handleFolderMouseLeave('f1'));
    expect(result.current.dragOverFolderId).toBe('f2');
  });
});

it('rejects the source folder and leaves the tip enabled after an aborted drag', () => {
  const { result } = renderHook(() => useFolderDragAndDrop([makeModelFile({ id: 'm1', folderId: 'source' })], [makeFolder({ id: 'source' })], { refreshFolders: vi.fn(), refreshFiles: vi.fn() }));
  act(() => result.current.onDragFileStart('m1'));
  act(() => result.current.handleFolderMouseEnter('source'));
  expect(result.current.dragOverFolderId).toBeNull();
  act(() => fireEvent.mouseUp(document));
  expect(invoke).not.toHaveBeenCalled();
  expect(localStorage.getItem('3mf-katalog-dnd-tip-dismissed')).toBeNull();
});

it('keeps the tip enabled after a failed move', async () => {
  vi.mocked(invoke).mockRejectedValue(new Error('move failed'));
  const errorLog = vi.spyOn(console, 'error').mockImplementation(() => {});
  try {
    const { result } = renderHook(() => useFolderDragAndDrop([makeModelFile({ id: 'm1' })], [makeFolder({ id: 'target' })], { refreshFolders: vi.fn(), refreshFiles: vi.fn() }));
    act(() => result.current.onDragFileStart('m1'));
    act(() => result.current.handleFolderMouseEnter('target'));
    await act(async () => fireEvent.mouseUp(document));
    expect(result.current.moveToast?.error).toBe(true);
    expect(localStorage.getItem('3mf-katalog-dnd-tip-dismissed')).toBeNull();
  } finally { errorLog.mockRestore(); }
});
