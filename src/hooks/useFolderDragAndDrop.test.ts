import { containsCatalogRoot } from '../lib/folderTree';
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
    vi.mocked(invoke).mockResolvedValue([{ id: 'm1', status: 'moved', fromFolderId: 'old', fromPath: '/Old' }]);
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
    expect(invoke).toHaveBeenCalledWith('move_files_to_folder', { fileIds: ['m1'], folderId: 'f1' });
    await waitFor(() => expect(result.current.moveToast).toMatchObject({ from: 'Model', to: '/Target', canUndo: true }));
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

it.each([false, true])('drops into a collection without moving the file (already present=%s)', async (present) => {
  const model = makeModelFile({ id: 'm1', name: 'Model' });
  vi.mocked(invoke).mockResolvedValue(present ? [model] : []);
  const addModelToCollection = vi.fn().mockResolvedValue(undefined);
  const { result } = renderHook(() => useFolderDragAndDrop([model], [],
    { refreshFolders: vi.fn(), refreshFiles: vi.fn() },
    { collections: [{ id: 'c1', name: 'Kitchen', modelCount: 0 }], addModelToCollection }));
  act(() => result.current.onDragFileStart('m1'));
  act(() => result.current.handleCollectionMouseEnter('c1'));
  expect(result.current.dragOverCollectionId).toBe('c1');
  await act(async () => fireEvent.mouseUp(document));
  expect(addModelToCollection).toHaveBeenCalledTimes(present ? 0 : 1);
  if (!present) expect(addModelToCollection).toHaveBeenCalledWith('m1', 'c1');
  expect(invoke).not.toHaveBeenCalledWith('move_file_to_folder', expect.anything());
  expect(result.current.moveToast).toMatchObject({ from: 'Model', to: 'Kitchen', collection: present ? 'already' : 'added' });
  expect(localStorage.getItem('3mf-katalog-dnd-tip-dismissed')).toBe('true');
});

it('aborts a collection drop outside and ignores stale leave events', async () => {
  const addModelToCollection = vi.fn();
  const { result } = renderHook(() => useFolderDragAndDrop([makeModelFile({ id: 'm1' })], [],
    { refreshFolders: vi.fn(), refreshFiles: vi.fn() },
    { collections: [], addModelToCollection }));
  act(() => result.current.onDragFileStart('m1'));
  act(() => result.current.handleCollectionMouseEnter('c1'));
  act(() => result.current.handleCollectionMouseEnter('c2'));
  act(() => result.current.handleCollectionMouseLeave('c1'));
  expect(result.current.dragOverCollectionId).toBe('c2');
  act(() => result.current.handleCollectionMouseLeave('c2'));
  await act(async () => fireEvent.mouseUp(document));
  expect(invoke).not.toHaveBeenCalled();
  expect(addModelToCollection).not.toHaveBeenCalled();
  expect(result.current.dragOverCollectionId).toBeNull();
  expect(localStorage.getItem('3mf-katalog-dnd-tip-dismissed')).toBeNull();
});

it('switches exclusively between folder and collection targets and ignores folder drags on collections', () => {
  const { result } = renderHook(() => useFolderDragAndDrop([makeModelFile({ id: 'm1' })], [makeFolder({ id: 'f1' })],
    { refreshFolders: vi.fn(), refreshFiles: vi.fn() }));
  act(() => result.current.onDragFileStart('m1'));
  act(() => result.current.handleFolderMouseEnter('f1'));
  act(() => result.current.handleCollectionMouseEnter('c1'));
  expect(result.current.dragOverFolderId).toBeNull();
  act(() => result.current.handleFolderMouseEnter('f1'));
  expect(result.current.dragOverCollectionId).toBeNull();
  act(() => result.current.handleFolderMouseLeave('f1'));
  act(() => fireEvent.mouseUp(document));
  act(() => result.current.onDragFolderStart('f1'));
  act(() => result.current.handleCollectionMouseEnter('c1'));
  expect(result.current.dragOverCollectionId).toBeNull();
  act(() => fireEvent.mouseUp(document));
  expect(invoke).not.toHaveBeenCalled();
});

it('keeps the drag tip after a failed collection add and uses the error toast', async () => {
  vi.mocked(invoke).mockResolvedValue([]);
  const addModelToCollection = vi.fn().mockRejectedValue(new Error('add failed'));
  const { result } = renderHook(() => useFolderDragAndDrop([makeModelFile({ id: 'm1' })], [],
    { refreshFolders: vi.fn(), refreshFiles: vi.fn() },
    { collections: [{ id: 'c1', name: 'Kitchen', modelCount: 0 }], addModelToCollection }));
  act(() => result.current.onDragFileStart('m1'));
  act(() => result.current.handleCollectionMouseEnter('c1'));
  await act(async () => fireEvent.mouseUp(document));
  expect(result.current.moveToast).toMatchObject({ error: true, unexpected: true });
  expect(localStorage.getItem('3mf-katalog-dnd-tip-dismissed')).toBeNull();
});

it.each(['root', 'parent'])('never moves the configured catalog %s and explains why', id => {
  const folders = [makeFolder({id: 'parent', path: '/models'}), makeFolder({id: 'root', path: '/models/catalog', parentId: 'parent'}), makeFolder({id: 'target', path: '/other'})];
  const {result} = renderHook(() => useFolderDragAndDrop([], folders, {refreshFolders: vi.fn(), refreshFiles: vi.fn()}, undefined, '/models/catalog'));
  act(() => result.current.onDragFolderStart(id));
  act(() => result.current.handleFolderMouseEnter('target'));
  expect(result.current.dragOverFolderId).toBeNull();
  act(() => { fireEvent.mouseUp(document); });
  expect(invoke).not.toHaveBeenCalled();
  expect(result.current.moveToast).toMatchObject({ error: true, blocked: 'catalogRoot' });
});

it('still moves a child of the configured root', async () => {
  vi.mocked(invoke).mockResolvedValue(undefined);
  const folders = [makeFolder({id: 'root', path: '/models'}), makeFolder({id: 'child', path: '/models/child', parentId: 'root'}), makeFolder({id: 'target', path: '/models/target', parentId: 'root'})];
  const {result} = renderHook(() => useFolderDragAndDrop([], folders, {refreshFolders: vi.fn(), refreshFiles: vi.fn()}, undefined, '/models'));
  act(() => result.current.onDragFolderStart('child'));
  act(() => result.current.handleFolderMouseEnter('target'));
  await act(async () => { fireEvent.mouseUp(document); });
  expect(invoke).toHaveBeenCalledWith('move_folder', {folderId: 'child', newParentId: 'target'});
});

it.each([
  ['/models', '/models/catalog', true],
  ['/models/catalog', '/models/catalog/', true],
  ['/models/cat', '/models/catalog', false],
  ['/models/catalog/child', '/models/catalog', false],
  ['/', '/models/catalog', true],
  ['C:\\Models', 'c:/models/catalog', true],
  ['C:/MODELS/CATALOG', 'c:/models/catalog', true],
  ['/Models', '/models/catalog', false],
  ['/models', '/models\\catalog', false],
])('checks catalog containment at path boundaries (%s, %s)', (path, root, expected) => {
  expect(containsCatalogRoot(path, root)).toBe(expected);
});

describe('refused folder drops', () => {
  const folders = [
    makeFolder({ id: 'root', name: 'Models', path: '/catalog/Models', parentId: null }),
    makeFolder({ id: 'a', name: 'A', path: '/catalog/Models/A', parentId: 'root' }),
    makeFolder({ id: 'a1', name: 'A1', path: '/catalog/Models/A/A1', parentId: 'a' }),
    makeFolder({ id: 'b', name: 'B', path: '/catalog/Models/B', parentId: 'root' }),
    makeFolder({ id: 'b-a1', name: 'A1', path: '/catalog/Models/B/A1', parentId: 'b' }),
  ];

  function dropOn(dragged: string, target: string, catalogBaseDir: string | null = '/catalog/Models/B') {
    const { result } = renderHook(() => useFolderDragAndDrop([], folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }, undefined, catalogBaseDir));
    act(() => result.current.onDragFolderStart(dragged));
    act(() => result.current.handleFolderMouseEnter(target));
    expect(result.current.dragOverFolderId).toBeNull();
    act(() => { fireEvent.mouseUp(document); });
    return result;
  }

  it.each([
    ['a', 'a', 'self'],
    ['a', 'a1', 'descendant'],
    ['a1', 'a', 'alreadyThere'],
    ['a1', 'b', 'nameTaken'],
    ['root', 'a', 'catalogRoot'],
    ['b', 'a', 'catalogRoot'],
  ])('explains why dropping %s on %s does nothing (%s)', (dragged, target, blocked) => {
    const result = dropOn(dragged, target);
    expect(invoke).not.toHaveBeenCalled();
    expect(result.current.moveToast).toMatchObject({ error: true, blocked });
    expect(result.current.draggedFolderId).toBeNull();
  });

  it('stays quiet when the pointer was released outside every folder row', () => {
    const { result } = renderHook(() => useFolderDragAndDrop([], folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }));
    act(() => result.current.onDragFolderStart('a'));
    act(() => result.current.handleFolderMouseEnter('b'));
    act(() => result.current.handleFolderMouseLeave('b'));
    act(() => { fireEvent.mouseUp(document); });
    expect(result.current.moveToast).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('still moves a folder onto a valid target', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { result } = renderHook(() => useFolderDragAndDrop([], folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }, undefined, null));
    act(() => result.current.onDragFolderStart('a1'));
    act(() => result.current.handleFolderMouseEnter('root'));
    expect(result.current.dragOverFolderId).toBe('root');
    await act(async () => { fireEvent.mouseUp(document); });
    expect(invoke).toHaveBeenCalledWith('move_folder', { folderId: 'a1', newParentId: 'root' });
  });

  it('shows the backend message of a failed move unchanged', async () => {
    vi.mocked(invoke).mockRejectedValue({ message: 'Zielordner existiert bereits: /x', expected: true });
    const errorLog = vi.spyOn(console, 'error').mockImplementation(() => {});
    try {
      const { result } = renderHook(() => useFolderDragAndDrop([], folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }, undefined, null));
      act(() => result.current.onDragFolderStart('a1'));
      act(() => result.current.handleFolderMouseEnter('root'));
      await act(async () => { fireEvent.mouseUp(document); });
      expect(result.current.moveToast).toMatchObject({ to: 'Zielordner existiert bereits: /x', error: true, unexpected: false });
    } finally { errorLog.mockRestore(); }
  });
});

describe('dragging a mixed selection', () => {
  const folders = [makeFolder({ id: 'a', path: '/A' }), makeFolder({ id: 'b', path: '/B' })];
  const models = [
    makeModelFile({ id: 'm1', name: 'One', folderId: 'a' }),
    makeModelFile({ id: 'm2', name: 'Two', folderId: 'b' }),
  ];

  it('keeps the folder of one dragged file a valid target while another file is elsewhere', async () => {
    vi.mocked(invoke).mockResolvedValue([
      { id: 'm1', status: 'skipped_already_there' },
      { id: 'm2', status: 'moved', fromFolderId: 'b', fromPath: '/B' },
    ]);
    const { result } = renderHook(() =>
      useFolderDragAndDrop(models, folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }, undefined, null, new Set(['m1', 'm2'])),
    );
    act(() => result.current.onDragFileStart('m1'));
    expect(result.current.draggedFilesFolderId).toBeNull();
    act(() => result.current.handleFolderMouseEnter('a'));
    expect(result.current.dragOverFolderId).toBe('a');
    await act(async () => {
      fireEvent.mouseUp(document);
    });
    expect(invoke).toHaveBeenCalledWith('move_files_to_folder', { fileIds: ['m1', 'm2'], folderId: 'a' });
  });

  it('refuses the target only when every dragged file is already in it', () => {
    const { result } = renderHook(() =>
      useFolderDragAndDrop(models, folders, { refreshFolders: vi.fn(), refreshFiles: vi.fn() }, undefined, null, new Set(['m1'])),
    );
    act(() => result.current.onDragFileStart('m1'));
    expect(result.current.draggedFilesFolderId).toBe('a');
    act(() => result.current.handleFolderMouseEnter('a'));
    expect(result.current.dragOverFolderId).toBeNull();
    act(() => result.current.handleFolderMouseEnter('b'));
    expect(result.current.dragOverFolderId).toBe('b');
  });
});
