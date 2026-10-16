import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { login, logout, authenticationFailed, canEditMetadata, useCanEditMetadata, changePassword, getSession } from '../lib/webSession';
import { WEB_READ_COMMANDS } from '../lib/readOnly';
import { invoke } from '../lib/transport';
import { useCatalogStore } from './useCatalogStore';
import { useCollections } from './useCollections';
import { usePrintLog } from './usePrintLog';
import { useBulkSelection } from './useBulkSelection';
import { useFolderDragAndDrop } from './useFolderDragAndDrop';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';
import { LanguageProvider } from '../i18n/LanguageContext';
import { makeModelFile, makeModelFileSummary, makeFolder } from '../test/factories';
import { MetadataError } from '../components/MetadataError';
import { clearMetadataError } from '../lib/metadataErrors';
import { ContextMenu } from '../components/ContextMenu';
import { readPrintLogPhoto, MAX_CUSTOM_IMAGE_BYTES } from '../lib/printLogPhoto';
// Explicit metadata contract, independent of the full write allowlist's order.
const METADATA_COMMANDS = [
  'set_favorite', 'set_print_status', 'add_tag', 'remove_tag', 'set_license',
  'set_source_url', 'add_print_log_entry', 'delete_print_log_entry', 'mark_file_viewed',
  'add_to_queue', 'remove_from_queue', 'reorder_queue', 'create_collection',
  'rename_collection', 'delete_collection', 'add_files_to_collection',
  'remove_file_from_collection', 'reorder_collection',
] as const;
const model = makeModelFile({ id: '1', tags: ['old'], queuePosition: null, favorite: false });
let commands: { command: string; args: Record<string, unknown> }[];
let writable = true;
let failing = new Set<string>();
let active = 0;
let peak = 0;
const fetchMock = vi.fn(async (url: string, options?: RequestInit): Promise<Response> => {
  if (!url.startsWith('/api/invoke/')) return Response.json({ authenticated: true, writable, csrfToken: 'token', mustChangePassword: false });
  const command = url.split('/').pop()!;
  const args = JSON.parse(options?.body as string || '{}');
  commands.push({ command, args });
  if (failing.has(command)) throw new TypeError('Network unavailable');
  active++; peak = Math.max(peak, active);
  await new Promise(resolve => setTimeout(resolve, 0)); active--;
  const results: Record<string, unknown> = {
    list_file_summaries: [makeModelFileSummary(model)], list_all_file_tags: { '1': ['old'] },
    list_files_by_ids: [model], list_collection_files: [model], list_collections: [{ id: '2', name: 'Collection', fileCount: 1 }],
    add_to_queue: 1, add_print_log_entry: { id: '3', printedAt: '2026-10-16T00:00:00.000Z', note: null, photoImage: null },
  };
  return Response.json(results[command] ?? []);
});
beforeEach(async () => {
  vi.stubEnv('VITE_TARGET', 'web'); vi.stubGlobal('fetch', fetchMock); fetchMock.mockClear();
  commands = []; failing = new Set(); writable = true; active = peak = 0; clearMetadataError();
  await login('password');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); vi.unstubAllEnvs(); });
const writes = () => commands.filter(({ command }) => !(WEB_READ_COMMANDS as readonly string[]).includes(command));
it.each([false, true])('dispatches the 18 metadata actions through their hooks only when writable=%s', async enabled => {
  writable = enabled; await getSession();
  const store = renderHook(() => useCatalogStore());
  const collections = renderHook(() => useCollections(), { wrapper: LanguageProvider });
  const log = renderHook(() => usePrintLog('1'));
  await waitFor(() => expect(store.result.current.initialLoading).toBe(false));
  act(() => collections.result.current.setActiveCollection('2'));
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 10)); });
  commands = [];
  const actions = [
    () => store.result.current.toggleFavorite('1'), () => store.result.current.togglePrintStatus('1'),
    () => store.result.current.addTag('1', 'new'), () => store.result.current.removeTag('1', 'old'),
    () => store.result.current.setModelLicenses(['1'], 'CC0-1.0'), () => store.result.current.setModelSourceUrl('1', 'https://example.org'),
    () => log.result.current.addEntry('2026-10-16T00:00:00.000Z', null, null), () => log.result.current.deleteEntry('3'),
    () => store.result.current.selectModel('1'), () => store.result.current.addToQueue('1'),
    () => store.result.current.removeFromQueue('1'), () => store.result.current.reorderQueue(['1']),
    () => collections.result.current.createCollection('new'), () => collections.result.current.renameCollection('2', 'new'),
    () => collections.result.current.addModelToCollection('1', '2'), () => collections.result.current.bulkRemoveFromCollection(['1']),
    () => collections.result.current.reorderCollection(['1']), () => collections.result.current.deleteCollection('2'),
  ];
  for (const action of actions) await act(async () => { await action(); await new Promise(resolve => setTimeout(resolve, 15)); });
  expect(writes().map(call => call.command).sort()).toEqual(enabled ? [...METADATA_COMMANDS].sort() : []);
  if (enabled) expect(writes().find(call => call.command === 'add_print_log_entry')?.args).toEqual({ fileId: '1', printedAt: '2026-10-16T00:00:00.000Z', note: null, photoBase64: null });
});
it('still blocks filesystem handlers, deletion shortcuts and folder drag while writable', async () => {
  const store = renderHook(() => useCatalogStore());
  const drag = renderHook(() => useFolderDragAndDrop([model], [makeFolder({ id: 'a' }), makeFolder({ id: 'b' })], { refreshFiles: vi.fn(), refreshFolders: vi.fn() }));
  const remove = vi.fn(); const select = vi.fn();
  renderHook(() => useKeyboardShortcuts({ filteredIds: ['1'], selectedId: '1', selectModel: vi.fn(), navigationEnabled: true, hasBulkSelection: true, openBulkDeleteConfirm: remove, toggleBulkSelect: select, selectAllVisible: select }));
  await waitFor(() => expect(store.result.current.initialLoading).toBe(false)); commands = [];
  await act(async () => {
    const s = store.result.current;
    await s.deleteModel('1'); await s.renameFile('1', 'new'); await s.restoreModel('1'); await s.deleteModelPermanently('1'); await s.emptyTrashAction();
    await s.uploadCustomImage('1'); await s.captureRenderSnapshot('1', 'x'); await s.rescanMetadata('1');
    await drag.result.current.onCreateFolder(null, 'new'); drag.result.current.onDragFolderStart('a'); drag.result.current.onDragFileStart('1');
  });
  act(() => drag.result.current.handleFolderMouseEnter('b'));
  fireEvent.mouseUp(document);
  for (const key of ['Delete', 'Backspace']) fireEvent.keyDown(window, { key });
  fireEvent.keyDown(window, { key: 'Backspace', metaKey: true });
  fireEvent.keyDown(window, { key: 'a', ctrlKey: true }); fireEvent.keyDown(window, { key: ' ' });
  expect(remove).not.toHaveBeenCalled(); expect(select).toHaveBeenCalled(); expect(writes()).toEqual([]);
  for (const command of ['delete_file', 'rename_file', 'restore_file', 'empty_trash', 'create_folder', 'move_file_to_folder', 'move_folder', 'start_import', 'rescan_file_metadata', 'upload_custom_image', 'set_render_snapshot', 'pick_and_read_image', 'remove_files_from_catalog']) {
    await expect(invoke(command)).rejects.toMatchObject({ expected: true });
  }
  expect(writes()).toEqual([]);
});
it('reactively resets capabilities on logout, 401 and password requirement; reads all three auth responses', async () => {
  const hook = renderHook(useCanEditMetadata); expect(hook.result.current).toBe(true);
  act(() => authenticationFailed(403, 'password_change_required')); expect(hook.result.current).toBe(false);
  await act(async () => changePassword('old', 'new')); expect(hook.result.current).toBe(true);
  await act(async () => logout()); expect(hook.result.current).toBe(false);
  await act(async () => login('password')); expect(hook.result.current).toBe(true);
  act(() => authenticationFailed(401)); expect(hook.result.current).toBe(false);
  writable = false; await act(async () => getSession()); expect(canEditMetadata()).toBe(false);
});
it('limits a 100-file bulk action to four requests and reconciles partial failures', async () => {
  const models = Array.from({ length: 100 }, (_, i) => makeModelFile({ id: String(i + 1) }));
  const patch = vi.fn();
  const bulk = renderHook(() => useBulkSelection({ models, setModels: patch, refreshFolders: vi.fn(), refreshTags: vi.fn(), refreshTrash: vi.fn(), refetchAfterPartialDelete: vi.fn(), bulkAddToCollection: vi.fn(), bulkRemoveFromCollection: vi.fn() }));
  act(() => bulk.result.current.selectAllVisible(models.map(m => m.id)));
  await act(async () => bulk.result.current.bulkSetPrintStatus('printed'));
  expect(writes()).toHaveLength(100); expect(peak).toBe(4); expect(patch).toHaveBeenCalled();
  failing.add('add_tag'); failing.add('list_files_by_ids');
  render(<LanguageProvider><MetadataError /></LanguageProvider>);
  await act(async () => bulk.result.current.bulkAddTagAction('new'));
  expect(screen.getByRole('alert')).toHaveTextContent('Network unavailable');
});
it('rolls back failed optimistic writes even when resync fails and shows a persistent error', async () => {
  const store = renderHook(() => useCatalogStore());
  render(<LanguageProvider><MetadataError /></LanguageProvider>);
  await waitFor(() => expect(store.result.current.initialLoading).toBe(false));
  failing.add('set_favorite'); failing.add('list_files_by_ids');
  act(() => store.result.current.toggleFavorite('1'));
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Network unavailable'));
  await waitFor(() => expect(store.result.current.models[0].favorite).toBe(false));
});
it('web context menus expose metadata but no file operations', () => {
  const action = vi.fn();
  render(<LanguageProvider><ContextMenu x={0} y={0} onClose={action} fileId="1" currentName="model.stl" inQueue={false} printed={false} onToggleQueue={action} onTogglePrintStatus={action} onDelete={action} onOpenInSlicer={action} onRename={action} /></LanguageProvider>);
  expect(screen.getAllByRole('button')).toHaveLength(3);
  expect(screen.queryByText('Löschen')).toBeNull();
});
it.each([429, 503])('retries %s once with Retry-After', async status => {
  fetchMock.mockResolvedValueOnce(Response.json({ message: 'busy' }, { status, headers: { 'Retry-After': '0' } }));
  await invoke('set_favorite', { id: '1', favorite: true });
  expect(fetchMock).toHaveBeenCalledTimes(3); // login + initial + retry
});
it('never retries an ambiguous network failure or a second busy response', async () => {
  fetchMock.mockRejectedValueOnce(new TypeError('offline'));
  await expect(invoke('create_collection')).rejects.toThrow('offline');
  expect(fetchMock).toHaveBeenCalledTimes(2);
  fetchMock.mockResolvedValueOnce(Response.json({ message: 'busy' }, { status: 503, headers: { 'Retry-After': '0' } }))
    .mockResolvedValueOnce(Response.json({ message: 'busy' }, { status: 503, headers: { 'Retry-After': '0' } }));
  await expect(invoke('create_collection')).rejects.toMatchObject({ message: 'busy' });
  expect(fetchMock).toHaveBeenCalledTimes(4);
});
it('rejects oversized photos before reading and unsupported formats before sending', async () => {
  const read = vi.fn();
  await expect(readPrintLogPhoto({ size: MAX_CUSTOM_IMAGE_BYTES + 1, arrayBuffer: read } as unknown as File)).rejects.toThrow('webPhotoTooLarge');
  expect(read).not.toHaveBeenCalled();
  await expect(readPrintLogPhoto({ size: 3, arrayBuffer: async () => new Uint8Array([1, 2, 3]).buffer } as File)).rejects.toThrow('webPhotoFormat');
  expect(await readPrintLogPhoto({ size: 3, arrayBuffer: async () => new Uint8Array([255, 216, 255]).buffer } as File)).toBe('/9j/');
  expect(writes()).toEqual([]);
});
it('reports failed queue additions instead of logging silently', async () => {
  const store = renderHook(() => useCatalogStore());
  render(<LanguageProvider><MetadataError /></LanguageProvider>);
  await waitFor(() => expect(store.result.current.initialLoading).toBe(false));
  failing.add('add_to_queue');
  act(() => store.result.current.addToQueue('1'));
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Network unavailable'));
});
it('does not undo a newer queue change when status write and resync fail', async () => {
  const store = renderHook(() => useCatalogStore());
  await waitFor(() => expect(store.result.current.initialLoading).toBe(false));
  let rejectStatus!: (reason: unknown) => void;
  fetchMock.mockImplementationOnce(() => new Promise((_resolve, reject) => { rejectStatus = reject; }));
  act(() => store.result.current.togglePrintStatus('1'));
  failing.add('list_files_by_ids');
  act(() => store.result.current.addToQueue('1'));
  await waitFor(() => expect(store.result.current.models[0].queuePosition).toBe(1));
  failing.add('list_files_by_ids');
  await act(async () => { rejectStatus(new Error('status failed')); await new Promise(resolve => setTimeout(resolve, 10)); });
  expect(store.result.current.models[0].printStatus).toBe('not_printed');
  expect(store.result.current.models[0].queuePosition).toBe(1);
});
it('revokes writable on HTTP 401 even with an invalid JSON body', async () => {
  fetchMock.mockResolvedValueOnce(new Response('', { status: 401 }));
  await expect(invoke('set_favorite')).rejects.toBeDefined();
  expect(canEditMetadata()).toBe(false);
});
it('sends bulk licenses as bounded individual requests, including more than 500 IDs', async () => {
  const store = renderHook(() => useCatalogStore());
  await waitFor(() => expect(store.result.current.initialLoading).toBe(false)); commands = []; peak = 0;
  await act(async () => store.result.current.setModelLicenses(Array.from({ length: 501 }, (_, i) => String(i + 1)), 'CC0-1.0'));
  expect(writes()).toHaveLength(501); expect(peak).toBeLessThanOrEqual(4);
  expect(writes().every(call => (call.args.fileIds as string[]).length === 1)).toBe(true);
});
it('does not restore write capability from a session response arriving after logout', async () => {
  let resolveSession!: (response: Response) => void;
  fetchMock.mockImplementationOnce(() => new Promise(resolve => { resolveSession = resolve; }));
  const pending = getSession();
  await logout();
  resolveSession(Response.json({ authenticated: true, writable: true, csrfToken: 'old', mustChangePassword: false }));
  await pending;
  expect(canEditMetadata()).toBe(false);
});
