import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, renderHook } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useFolderDragAndDrop } from './useFolderDragAndDrop';
import { makeFolder, makeModelFile } from '../test/factories';
import { MAX_UNDO_STEPS } from '../lib/moveUndo';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));

const folders = [
  makeFolder({ id: 'a', name: 'A', path: '/c/A' }),
  makeFolder({ id: 'b', name: 'B', path: '/c/B' }),
  makeFolder({ id: 'target', name: 'Target', path: '/c/Target' }),
];
const models = [
  makeModelFile({ id: 'm1', name: 'One.3mf', folderId: 'a' }),
  makeModelFile({ id: 'm2', name: 'Two.3mf', folderId: 'a' }),
  makeModelFile({ id: 'm3', name: 'Three.3mf', folderId: 'b' }),
  makeModelFile({ id: 'm4', name: 'Four.3mf', folderId: 'b' }),
];

// Backend stand-in: every file moves and reports the folder it came from.
function backendMovesEverything() {
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd !== 'move_files_to_folder') return undefined;
    const { fileIds } = args as { fileIds: string[] };
    return fileIds.map((id) => ({ id, status: 'moved', fromFolderId: models.find((m) => m.id === id)?.folderId, fromPath: '/old' }));
  });
}

function setup(selected: string[] = []) {
  const refreshFolders = vi.fn();
  const refreshFiles = vi.fn();
  const rendered = renderHook(() =>
    useFolderDragAndDrop(models, folders, { refreshFolders, refreshFiles }, undefined, null, new Set(selected)));
  return { ...rendered, refreshFolders, refreshFiles };
}
type Hook = ReturnType<typeof setup>['result'];

async function dropFile(result: Hook, id: string, folderId: string) {
  act(() => result.current.onDragFileStart(id));
  act(() => result.current.handleFolderMouseEnter(folderId));
  await act(async () => { fireEvent.mouseUp(document); });
}
const calls = (cmd: string) => vi.mocked(invoke).mock.calls.filter(([name]) => name === cmd).map(([, args]) => args);

beforeEach(() => { vi.mocked(invoke).mockReset(); localStorage.clear(); });

describe('moving several files by dragging', () => {
  it('drags the whole selection when the grabbed card is part of it', async () => {
    backendMovesEverything();
    const { result } = setup(['m1', 'm3', 'm4']);
    act(() => result.current.onDragFileStart('m3'));
    expect(result.current.draggedFileIds.sort()).toEqual(['m1', 'm3', 'm4']);
    act(() => result.current.handleFolderMouseEnter('target'));
    await act(async () => { fireEvent.mouseUp(document); });
    expect(calls('move_files_to_folder')).toEqual([{ fileIds: ['m1', 'm3', 'm4'], folderId: 'target' }]);
    expect(result.current.draggedFileIds).toEqual([]);
  });

  it('moves only the grabbed card when it is not selected', async () => {
    backendMovesEverything();
    const { result } = setup(['m1', 'm2']);
    await dropFile(result, 'm4', 'target');
    expect(calls('move_files_to_folder')).toEqual([{ fileIds: ['m4'], folderId: 'target' }]);
  });

  it('does not offer a collection as target for a multi-card drag', () => {
    const { result } = setup(['m1', 'm2']);
    act(() => result.current.onDragFileStart('m1'));
    act(() => result.current.handleCollectionMouseEnter('c1'));
    expect(result.current.dragOverCollectionId).toBeNull();
  });

  it('reports partial results per reason and keeps undo only for the moved files', async () => {
    vi.mocked(invoke).mockResolvedValue([
      { id: 'm1', status: 'moved', fromFolderId: 'a' },
      { id: 'm2', status: 'skipped', reason: 'name_exists' },
      { id: 'm3', status: 'failed', reason: 'source_missing' },
    ]);
    const { result } = setup(['m1', 'm2', 'm3']);
    await dropFile(result, 'm1', 'target');
    expect(result.current.moveToast).toMatchObject({
      to: '/c/Target', canUndo: true,
      batch: { names: ['One.3mf'], total: 3, summary: { moved: 1, skippedNameExists: 1, failed: 1, firstFailReason: 'source_missing' } },
    });
    expect(result.current.undoCount).toBe(1);
  });

  it('offers no undo for files whose old folder is unknown', async () => {
    vi.mocked(invoke).mockResolvedValue([{ id: 'm1', status: 'moved' }]);
    const { result } = setup();
    await dropFile(result, 'm1', 'target');
    expect(result.current.moveToast?.canUndo).toBe(false);
    expect(result.current.undoCount).toBe(0);
  });
});

describe('undo', () => {
  it('puts files back grouped by their old folder, newest step first', async () => {
    backendMovesEverything();
    const { result, refreshFiles } = setup(['m1', 'm2', 'm3']);
    await dropFile(result, 'm1', 'target');
    vi.mocked(invoke).mockClear();
    await act(async () => { await result.current.undo(); });
    expect(calls('move_files_to_folder')).toEqual([
      { fileIds: ['m1', 'm2'], folderId: 'a' },
      { fileIds: ['m3'], folderId: 'b' },
    ]);
    expect(result.current.moveToast).toMatchObject({ undo: { kind: 'files', ok: true, count: 3, total: 3 } });
    expect(result.current.undoCount).toBe(0);
    expect(refreshFiles).toHaveBeenCalledTimes(2);
  });

  it('moves a folder back to the parent it had before', async () => {
    const tree = [
      makeFolder({ id: 'p1', name: 'P1', path: '/c/P1' }),
      makeFolder({ id: 'c1', name: 'Child', path: '/c/P1/Child', parentId: 'p1' }),
      makeFolder({ id: 'p2', name: 'P2', path: '/c/P2' }),
    ];
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = renderHook(() => useFolderDragAndDrop([], tree, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }));
    act(() => result.current.onDragFolderStart('c1'));
    act(() => result.current.handleFolderMouseEnter('p2'));
    await act(async () => { fireEvent.mouseUp(document); });
    expect(invoke).toHaveBeenCalledWith('move_folder', { folderId: 'c1', newParentId: 'p2' });
    expect(result.current.moveToast?.canUndo).toBe(true);
    await act(async () => { await result.current.undo(); });
    expect(invoke).toHaveBeenLastCalledWith('move_folder', { folderId: 'c1', newParentId: 'p1' });
    expect(result.current.moveToast?.undo).toMatchObject({ kind: 'folderMove', ok: true, name: 'Child' });
  });

  it('renames a folder back to its old name', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = setup();
    act(() => result.current.recordFolderRename({ folderId: 'a', oldName: 'Parts', newName: 'Druckerteile' }));
    await act(async () => { await result.current.undo(); });
    expect(invoke).toHaveBeenCalledWith('rename_folder', { folderId: 'a', name: 'Parts' });
    expect(result.current.moveToast?.undo).toMatchObject({ kind: 'rename', ok: true, name: 'Druckerteile', restoredName: 'Parts' });
  });

  it('reports a refused undo, drops the step and changes nothing else', async () => {
    const errorLog = vi.spyOn(console, 'error').mockImplementation(() => {});
    try {
      const { result, refreshFolders } = setup();
      act(() => result.current.recordFolderRename({ folderId: 'a', oldName: 'Parts', newName: 'New' }));
      act(() => result.current.recordFolderRename({ folderId: 'b', oldName: 'Old', newName: 'Newer' }));
      vi.mocked(invoke).mockRejectedValueOnce({ message: 'renameTargetFolderExists', expected: true, params: ['/p/Old'] });
      await act(async () => { await result.current.undo(); });
      expect(result.current.moveToast).toMatchObject({ error: true, undo: { ok: false, failure: { message: 'renameTargetFolderExists', params: ['/p/Old'], unexpected: false } } });
      expect(result.current.undoCount).toBe(1);
      expect(refreshFolders).not.toHaveBeenCalled();
      // The step below is still reachable.
      vi.mocked(invoke).mockResolvedValue(undefined);
      await act(async () => { await result.current.undo(); });
      expect(invoke).toHaveBeenLastCalledWith('rename_folder', { folderId: 'a', name: 'Parts' });
    } finally { errorLog.mockRestore(); }
  });

  it('reports a file undo that the backend skips because the old place is taken', async () => {
    backendMovesEverything();
    const { result } = setup();
    await dropFile(result, 'm1', 'target');
    vi.mocked(invoke).mockResolvedValue([{ id: 'm1', status: 'skipped', reason: 'name_exists' }]);
    await act(async () => { await result.current.undo(); });
    expect(result.current.moveToast?.undo).toMatchObject({ kind: 'files', ok: false, count: 0, total: 1, reason: 'name_exists' });
  });

  it('keeps at most 20 steps and drops the oldest', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = setup();
    for (let i = 0; i < MAX_UNDO_STEPS + 5; i++) {
      act(() => result.current.recordFolderRename({ folderId: 'a', oldName: `old${i}`, newName: `new${i}` }));
    }
    expect(result.current.undoCount).toBe(20);
    for (let i = 0; i < 20; i++) await act(async () => { await result.current.undo(); });
    expect(invoke).toHaveBeenCalledTimes(20);
    expect(invoke).toHaveBeenNthCalledWith(1, 'rename_folder', { folderId: 'a', name: 'old24' });
    expect(invoke).toHaveBeenLastCalledWith('rename_folder', { folderId: 'a', name: 'old5' });
    await act(async () => { await result.current.undo(); });
    expect(invoke).toHaveBeenCalledTimes(20);
  });

  it('forgets all steps on clearUndo', async () => {
    const { result } = setup();
    act(() => result.current.recordFolderRename({ folderId: 'a', oldName: 'x', newName: 'y' }));
    act(() => result.current.clearUndo());
    expect(result.current.undoCount).toBe(0);
    await act(async () => { await result.current.undo(); });
    expect(invoke).not.toHaveBeenCalled();
  });
});
