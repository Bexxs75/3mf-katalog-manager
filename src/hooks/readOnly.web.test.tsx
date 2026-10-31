import { act, cleanup, fireEvent, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
vi.mock('../lib/transport', () => ({ invoke: vi.fn() }));
import { invoke } from '../lib/transport';
import { LanguageProvider } from '../i18n/LanguageContext';
import { useCatalogStore } from './useCatalogStore';
import { useCollections } from './useCollections';
import { useFolderDragAndDrop } from './useFolderDragAndDrop';
import { useBulkSelection } from './useBulkSelection';
import { usePrintLog } from './usePrintLog';
import { useEditableSourceUrl } from './useEditableSourceUrl';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';
import { makeFolder, makeModelFile, makeModelFileSummary } from '../test/factories';
const model = makeModelFile({ id: '1', tags: ['old'], folderId: 'root', lastViewedAt: '2026-10-01T00:00:00Z' });
beforeEach(() => {
  vi.stubEnv('VITE_TARGET', 'web');
  vi.mocked(invoke).mockImplementation(async command => command === 'list_file_summaries' ? [makeModelFileSummary(model)] : command === 'list_all_file_tags' ? { '1': ['old'] } : command === 'list_files_by_ids' ? [model] : []);
});
afterEach(() => { cleanup(); vi.clearAllMocks(); vi.unstubAllEnvs(); });
it('blocks every catalog mutation before IPC and before optimistic state changes, including viewed time and snapshots', async () => {
  const { result } = renderHook(() => useCatalogStore());
  await waitFor(() => expect(result.current.initialLoading).toBe(false));
  await act(async () => result.current.selectModel('1'));
  const before = result.current.models;
  vi.mocked(invoke).mockClear();
  await act(async () => {
    const s = result.current;
    s.selectModel('1');
    await s.restoreModel('1'); await s.deleteModelPermanently('1'); await s.emptyTrashAction();
    await s.addTag('1', 'new'); await s.removeTag('1', 'old'); await s.renameFile('1', 'changed.stl'); await s.deleteModel('1');
    await s.togglePrintStatus('1'); await s.toggleFavorite('1'); await s.addToQueue('1'); await s.removeFromQueue('1'); await s.reorderQueue(['1']);
    await s.uploadCustomImage('1'); await s.captureRenderSnapshot('1', 'data'); await s.setModelLicenses(['1'], 'CC0-1.0'); await s.setModelSourceUrl('1', 'https://example.org'); await s.rescanMetadata('1');
  });
  expect(result.current.selectedId).toBe('1');
  expect(result.current.models).toEqual(before);
  expect(vi.mocked(invoke).mock.calls.map(([command]) => command)).not.toEqual(expect.arrayContaining(['mark_file_viewed']));
  expect(vi.mocked(invoke).mock.calls.every(([command]) => command === 'list_files_by_ids')).toBe(true);
});
it('blocks collection CRUD, membership and reorder even when handlers are called directly', async () => {
  const { result } = renderHook(() => useCollections(), { wrapper: LanguageProvider });
  await act(async () => {});
  act(() => result.current.setActiveCollection('c'));
  await act(async () => {}); vi.mocked(invoke).mockClear();
  await act(async () => {
    const c = result.current;
    await c.createCollection('new'); await c.renameCollection('c', 'new'); await c.deleteCollection('c');
    await c.addModelToCollection('1', 'c'); await c.bulkAddToCollection(['1'], 'c'); await c.bulkRemoveFromCollection(['1']); await c.reorderCollection(['1']);
  });
  expect(invoke).not.toHaveBeenCalled(); expect(result.current.activeCollection).toBe('c');
});
it('blocks folder creation and drag starts, so mouseup cannot move models or folders', async () => {
  const refresh = vi.fn(); const add = vi.fn();
  const { result } = renderHook(() => useFolderDragAndDrop([model], [makeFolder({ id: 'root' }), makeFolder({ id: 'other' })], { refreshFolders: refresh, refreshFiles: refresh }, { collections: [], addModelToCollection: add }));
  await act(async () => {
    await result.current.onCreateFolder(null, 'new'); result.current.onDragFileStart('1'); result.current.onDragFolderStart('root');
  });
  act(() => { result.current.handleFolderMouseEnter('other'); result.current.handleCollectionMouseEnter('c'); fireEvent.mouseUp(document); });
  expect(result.current.draggedFileId).toBeNull(); expect(result.current.draggedFolderId).toBeNull();
  expect(invoke).not.toHaveBeenCalled(); expect(add).not.toHaveBeenCalled(); expect(refresh).not.toHaveBeenCalled();
});
it('blocks bulk selection and all bulk handlers without mutating local models', async () => {
  const mutation = vi.fn();
  const { result } = renderHook(() => useBulkSelection({ models: [model], setModels: mutation, refreshFolders: mutation, refreshTags: mutation, refreshTrash: mutation, refetchAfterPartialDelete: mutation, bulkAddToCollection: mutation, bulkRemoveFromCollection: mutation }));
  await act(async () => {
    const b = result.current;
    b.toggleBulkSelect('1'); b.selectAllVisible(['1']);
    await b.bulkDelete(); await b.bulkAddToQueue(); await b.bulkSetPrintStatus('printed');
    await b.bulkAddTagAction('new'); await b.bulkRemoveTagAction('old'); await b.bulkAddToCollectionAction('c'); await b.bulkRemoveFromCollectionAction();
  });
  expect(result.current.selectedForBulk.size).toBe(0); expect(invoke).not.toHaveBeenCalled(); expect(mutation).not.toHaveBeenCalled();
});
it('reads the print log but never adds or deletes entries', async () => {
  const { result } = renderHook(() => usePrintLog('1'));
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(invoke).toHaveBeenCalledWith('list_print_log_entries', { fileId: '1' }); vi.mocked(invoke).mockClear();
  await act(async () => { await result.current.addEntry('2026-10-01', 'note', null); await result.current.deleteEntry('e'); });
  expect(invoke).not.toHaveBeenCalled(); expect(result.current.entries).toEqual([]);
});
it('never starts source editing or saves on blur', () => {
  const save = vi.fn(); const { result } = renderHook(() => useEditableSourceUrl(model, save));
  act(() => { result.current.startEditing(); result.current.setDraft('https://example.org'); });
  act(() => result.current.handleBlur());
  expect(result.current.editing).toBe(false); expect(save).not.toHaveBeenCalled();
});
it('ignores mutation shortcuts even with stale bulk state but retains detail navigation', () => {
  const mutate = vi.fn(); const open = vi.fn();
  renderHook(() => useKeyboardShortcuts({ filteredIds: ['1'], selectedId: '1', selectModel: vi.fn(), hasBulkSelection: true, openBulkDeleteConfirm: mutate, toggleBulkSelect: mutate, selectAllVisible: mutate, navigationEnabled: true, onOpenDetail: open }));
  for (const key of ['Delete', 'Backspace', ' ']) fireEvent.keyDown(window, { key });
  fireEvent.keyDown(window, { key: 'a', ctrlKey: true }); fireEvent.keyDown(window, { key: 'Backspace', metaKey: true });
  fireEvent.keyDown(window, { key: 'Enter' });
  expect(mutate).not.toHaveBeenCalled(); expect(open).toHaveBeenCalledWith('1');
});
it('fileOps enables rename and the delete confirmation shortcut, then revokes both', async () => {
  const { getSession, authenticationFailed } = await import('../lib/webSession');
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({authenticated:true,writable:true,fileOps:true,csrfToken:'token'})));
  await getSession();
  const { result } = renderHook(() => useCatalogStore());
  await waitFor(() => expect(result.current.initialLoading).toBe(false));
  const confirm = vi.fn();
  renderHook(() => useKeyboardShortcuts({ filteredIds:['1'], selectedId:'1', selectModel:vi.fn(), hasBulkSelection:true, openBulkDeleteConfirm:confirm, navigationEnabled:true, toggleBulkSelect:vi.fn(), singleKeyShortcuts:true }));
  await act(async () => result.current.renameFile('1','new'));
  expect(invoke).toHaveBeenCalledWith('rename_file',{fileId:'1',name:'new'});
  fireEvent.keyDown(window,{key:'Delete'}); expect(confirm).toHaveBeenCalledOnce();
  authenticationFailed(401); vi.mocked(invoke).mockClear();
  await act(async () => result.current.renameFile('1','again'));
  fireEvent.keyDown(window,{key:'Delete'});
  expect(invoke).not.toHaveBeenCalled(); expect(confirm).toHaveBeenCalledOnce(); vi.unstubAllGlobals();
});
it('sends only the base name when a full file name reaches the web rename', async () => {
  const { getSession, authenticationFailed } = await import('../lib/webSession');
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({authenticated:true,writable:true,fileOps:true,csrfToken:'token'})));
  await getSession();
  const { result } = renderHook(() => useCatalogStore());
  await waitFor(() => expect(result.current.initialLoading).toBe(false));
  await act(async () => result.current.renameFile('1','x.stl'));
  await act(async () => result.current.renameFile('1','y.gcode.3mf'));
  expect(invoke).toHaveBeenCalledWith('rename_file',{fileId:'1',name:'x'});
  expect(invoke).toHaveBeenCalledWith('rename_file',{fileId:'1',name:'y'});
  authenticationFailed(401); vi.unstubAllGlobals();
});
