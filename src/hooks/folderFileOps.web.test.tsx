import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, renderHook } from '@testing-library/react';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { useFolderDragAndDrop } from './useFolderDragAndDrop';
import { authenticationFailed, getSession } from '../lib/webSession';
import { createFolder, renameFolder, moveFolder } from '../lib/api/folders';
import { makeFolder, makeModelFile } from '../test/factories';
let enabled: boolean;
let calls: { command: string; args: unknown }[];
beforeEach(async () => {
  enabled = true; calls = [];
  vi.stubGlobal('confirm', vi.fn(() => true));
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (url === '/api/session') return Response.json({ authenticated: true, writable: true, fileOps: enabled, csrfToken: 'token' });
    const command = url.split('/').pop()!;
    calls.push({ command, args: JSON.parse(String(options?.body)) });
    return Response.json(command === 'move_files_to_folder' ? [{ id: '1', ok: true }] : null);
  }));
  await getSession();
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); });
const folders = [makeFolder({ id: '2', name: 'source', path: '/private/source' }), makeFolder({ id: '3', name: 'target', path: '/private/target' })];
function setup() {
  const refresh = vi.fn();
  const hook = renderHook(() => useFolderDragAndDrop([makeModelFile({ id: '1', folderId: '2' })], folders, { refreshFolders: refresh, refreshFiles: refresh }));
  return { ...hook, refresh };
}
it('requires confirmation for web file movement and uses a batch with IDs only', async () => {
  const { result, refresh } = setup();
  act(() => result.current.onDragFileStart('1'));
  act(() => result.current.handleFolderMouseEnter('3'));
  await act(async () => { fireEvent.mouseUp(document); });
  expect(confirm).toHaveBeenCalledOnce();
  expect(calls).toEqual([{ command: 'move_files_to_folder', args: { fileIds: ['1'], folderId: '3' } }]);
  expect(refresh).not.toHaveBeenCalled();
});
it('cancelled moves and capability revocation send no mutations', async () => {
  const { result } = setup();
  vi.mocked(confirm).mockReturnValue(false);
  act(() => result.current.onDragFileStart('1'));
  act(() => result.current.handleFolderMouseEnter('3'));
  await act(async () => { fireEvent.mouseUp(document); });
  expect(calls).toEqual([]);
  act(() => result.current.onDragFolderStart('2'));
  act(() => result.current.handleFolderMouseEnter('3'));
  await act(async () => { enabled = false; await getSession(); });
  expect(result.current.draggedFolderId).toBeNull();
  await act(async () => { fireEvent.mouseUp(document); await result.current.onCreateFolder('2', 'child'); });
  expect(calls).toEqual([]);
});
it('folder moves include uncatalogued contents warning and newParentId', async () => {
  const { result } = setup();
  act(() => result.current.onDragFolderStart('2'));
  act(() => result.current.handleFolderMouseEnter('3'));
  await act(async () => { fireEvent.mouseUp(document); });
  expect(confirm).toHaveBeenCalledWith(expect.stringContaining('nicht katalogisierte'));
  expect(calls).toEqual([{ command: 'move_folder', args: { folderId: '2', newParentId: '3' } }]);
});
it('folder API sends create, rename and move contracts and refuses root creation', async () => {
  await expect(createFolder(null, 'root')).rejects.toBeDefined();
  expect(calls).toEqual([]);
  await createFolder('2', 'child'); await renameFolder('2', 'renamed'); await moveFolder('2', '3');
  expect(calls).toEqual([
    { command: 'create_folder', args: { parentId: '2', name: 'child' } },
    { command: 'rename_folder', args: { folderId: '2', name: 'renamed' } },
    { command: 'move_folder', args: { folderId: '2', newParentId: '3' } },
  ]);
});
it('rechecks permission after the move confirmation returns', async () => {
  const { result } = setup();
  vi.mocked(confirm).mockImplementation(() => { authenticationFailed(401); return true; });
  act(() => result.current.onDragFileStart('1'));
  act(() => result.current.handleFolderMouseEnter('3'));
  await act(async () => { fireEvent.mouseUp(document); });
  expect(calls).toEqual([]);
});
it('shows a translated failed-item result without exposing a raw server path', async () => {
  const { result } = setup();
  vi.mocked(fetch).mockResolvedValue(Response.json([{ id: '1', ok: false, kind: 'cross_device', message: '/private/server/path' }]));
  act(() => result.current.onDragFileStart('1'));
  act(() => result.current.handleFolderMouseEnter('3'));
  await act(async () => { fireEvent.mouseUp(document); });
  expect(result.current.moveToast?.error).toBe(true);
  expect(result.current.moveToast?.to).toBe('Verschieben ist nur innerhalb einer Wurzel und desselben Dateisystems möglich.');
  expect(result.current.moveToast?.to).not.toContain('/private');
});
