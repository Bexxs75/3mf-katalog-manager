import { beforeEach, expect, it, vi } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useFileImport } from './useFileImport';
import type { ImportJobResult, ImportProgress } from '../types';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const events = vi.hoisted(() => ({ handlers: {} as Record<string, (event: {payload: unknown}) => void>, unlisten: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn((name, cb) => { events.handlers[name] = cb; return Promise.resolve(events.unlisten); }) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: (cb: (e: {payload: unknown}) => void) => { events.handlers.drop = cb; return Promise.resolve(events.unlisten); } }) }));
export const counts = { imported: 1, importedNotPlaced: 0, duplicate: 0, skipped: 0, archive: 0, known: 1 };
const progress = (jobId = 'j1', state: ImportProgress['state'] = 'importing'): ImportProgress => ({ jobId, state, counts, scanComplete: true, done: 1, total: 2, inFlight: 1, current: 'a.stl', elapsedMs: 250 });
const finished = (state: ImportJobResult['state'] = 'finished'): ImportJobResult => ({ jobId: 'j1', source: 'files', state, parentJobId: null, jobError: null, scanComplete: true, placementRequired: true, counts, groups: { imported: [], importedNotPlaced: [], duplicate: [], skipped: [], archive: [] } });
beforeEach(() => {
  sessionStorage.clear(); vi.clearAllMocks(); events.handlers = {};
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'start_import' ? { jobId: 'j1' } : null);
});
function setup(folder = 'first') {
  const refreshFiles = vi.fn(), refreshFolders = vi.fn(), onImported = vi.fn();
  const hook = renderHook(({ folder }) => useFileImport({ enabled: true, catalogBaseDir: '/base', activeFolderId: folder, onImported, refreshFiles, refreshFolders }), { initialProps: { folder } });
  return { ...hook, refreshFiles, refreshFolders };
}
it('starts a job with the captured target and updates from progress', async () => {
  const { result, rerender } = setup();
  await act(async () => { await result.current.importFiles(); });
  rerender({ folder: 'second' });
  expect(invoke).toHaveBeenCalledWith('start_import', { source: 'files', targetFolderId: 'first' });
  act(() => events.handlers['import://progress']({ payload: progress() }));
  expect(result.current.progress?.current).toBe('a.stl'); expect(result.current.jobActive).toBe(true);
});
it.each(['finished', 'cancelled', 'failed'] as const)('%s refreshes both catalog lists', async state => {
  const { result, refreshFiles, refreshFolders } = setup();
  await act(async () => events.handlers['import://finished']({payload: finished(state)}));
  expect(result.current.result?.state).toBe(state); expect(refreshFiles).toHaveBeenCalledOnce(); expect(refreshFolders).toHaveBeenCalledOnce();
});
it('counts queued jobs independently and cancels the running job', async () => {
  const { result } = setup();
  act(() => { events.handlers['import://progress']({payload: progress()}); events.handlers['import://progress']({payload: progress('j2', 'queued')}); });
  expect(result.current.queued).toBe(1);
  await act(async () => { await result.current.cancel(); });
  expect(invoke).toHaveBeenCalledWith('cancel_import', {jobId: 'j1'});
});
it('restores a missed terminal event from session storage', async () => {
  sessionStorage.setItem('catalog-import-jobs', JSON.stringify([{ jobId: 'j1', source: 'files' }]));
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'get_import_state' ? progress('j1', 'finished') : cmd === 'get_import_result' ? finished() : null);
  const { result, refreshFiles } = setup();
  await waitFor(() => expect(result.current.result?.state).toBe('finished'));
  expect(invoke).toHaveBeenCalledWith('get_import_state', {jobId: 'j1'}); expect(refreshFiles).toHaveBeenCalledOnce();
});
it('opens pending archives and discards grants on cancellation', async () => {
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'inspect_archives' ? [{path: '/a.zip', status: 'ok'}] : null);
  const { result } = setup(); const value = finished(); value.groups.archive = [{path: '/a.zip', state: 'pending', grantId: 'g'}];
  await act(async () => events.handlers['import://finished']({payload: value}));
  expect(result.current.pendingArchives).toEqual([{path: '/a.zip', status: 'ok'}]);
  await act(async () => result.current.cancelArchives());
  expect(invoke).toHaveBeenCalledWith('discard_archive_imports', {paths: ['/a.zip']}); expect(result.current.pendingArchives).toBeNull();
});
it('unsubscribes listeners on unmount', async () => {
  const { unmount } = setup(); await act(async () => {}); unmount(); await act(async () => {});
  expect(events.unlisten).toHaveBeenCalledTimes(3);
});
it('uses the root folder when all models is active', async () => {
  vi.mocked(invoke).mockResolvedValue({jobId:null});
  const args = {enabled:true,catalogBaseDir:'/base',activeFolderId:'all',rootFolderId:'root',onImported:vi.fn(),refreshFiles:vi.fn(),refreshFolders:vi.fn()};
  const {result} = renderHook(() => useFileImport(args));
  await act(async () => result.current.importFiles());
  expect(invoke).toHaveBeenCalledWith('start_import',{source:'files',targetFolderId:'root'});
});
it('starts files without a root and folder imports without a placement target', async () => {
  vi.mocked(invoke).mockResolvedValue({jobId:null});
  const {result} = renderHook(() => useFileImport({enabled:true,catalogBaseDir:null,activeFolderId:'all',onImported:vi.fn(),refreshFiles:vi.fn(),refreshFolders:vi.fn()}));
  await act(async () => {await result.current.importFiles(); await result.current.importFolder();});
  expect(invoke).toHaveBeenCalledWith('start_import',{source:'files',targetFolderId:undefined}); expect(invoke).toHaveBeenCalledWith('start_import',{source:'folder',targetFolderId:undefined});
});
it('appends archives without duplicates and submits each parent separately', async () => {
  vi.mocked(invoke).mockImplementation(async (cmd,args) => cmd === 'inspect_archives' ? (args as {paths:string[]}).paths.map(path => ({path,status:'ok'})) : cmd === 'start_archive_import' ? {jobId:null} : null);
  const {result} = setup(); const first=finished(), second={...finished(),jobId:'j2'};
  first.groups={...first.groups,archive:[{path:'/a.zip',state:'pending',grantId:'a'}]}; second.groups={...second.groups,archive:[{path:'/a.zip',state:'pending',grantId:'a'},{path:'/b.zip',state:'pending',grantId:'b'}]};
  await act(async () => { await events.handlers['import://finished']({payload:first}); });
  await act(async () => { await events.handlers['import://finished']({payload:second}); });
  expect(result.current.pendingArchives).toHaveLength(2);
  const requests = ['/a.zip','/b.zip'].map(path => ({path,folderName:'folder',onConflict:'new' as const,expectedSize:1,expectedModifiedUnixMs:1}));
  await act(async () => result.current.startArchives('/target',requests,false));
  expect(invoke).toHaveBeenCalledWith('start_archive_import',{targetDir:'/target',requests:[requests[0]],deleteArchives:false,parentJobId:'j1'});
  expect(invoke).toHaveBeenCalledWith('start_archive_import',{targetDir:'/target',requests:[requests[1]],deleteArchives:false,parentJobId:'j2'});
  expect(result.current.pendingArchives).toBeNull();
});
it('shows inspection errors and keeps the terminal archive result', async () => {
  vi.mocked(invoke).mockRejectedValue({message:'inspection failed',expected:true}); const {result}=setup(); const value=finished(); value.groups.archive=[{path:'/a.zip',state:'pending',grantId:'a'}];
  await act(async () => events.handlers['import://finished']({payload:value}));
  expect(result.current.error?.message).toBe('inspection failed'); expect(result.current.result).toEqual(value);
});
it('keeps setup on the legacy summary path', () => {
  const {result}=setup(); act(() => result.current.mergeImported({imported:[],duplicateCount:2,skipped:[]}));
  expect(result.current.importBanner?.duplicates).toBe(2); act(() => result.current.dismissImportBanner()); expect(result.current.importBanner).toBeNull();
});
it('retains the captured source and target when finish arrives before start returns', async () => {
  let resolve!: (value: {jobId:string}) => void;
  vi.mocked(invoke).mockImplementation(cmd => cmd === 'start_import' ? new Promise(r => {resolve=r;}) : Promise.resolve(null));
  const {result,rerender}=setup(); let pending!: Promise<void>;
  act(() => {pending=result.current.importFiles();}); rerender({folder:'second'});
  await act(async () => events.handlers['import://finished']({payload:finished()}));
  await act(async () => {resolve({jobId:'j1'});await pending;});
  expect(result.current.result?.state).toBe('finished'); expect(result.current.meta?.targetName).toBe('/base'); expect(result.current.jobActive).toBe(false);
  expect(invoke).toHaveBeenCalledWith('start_import',{source:'files',targetFolderId:'first'});
});
it('deduplicates terminal events and removes the terminal row on dismissal', async () => {
  const {result,refreshFiles}=setup();
  await act(async () => {events.handlers['import://finished']({payload:finished()});events.handlers['import://finished']({payload:finished()});});
  expect(refreshFiles).toHaveBeenCalledOnce(); act(() => result.current.dismiss()); expect(result.current.progress).toBeNull(); expect(result.current.result).toBeNull();
});
it('accepts dropped paths during another import and ignores drops in other views', async () => {
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'start_dropped_import' ? {jobId:'j2'} : cmd === 'get_import_state' ? progress('j2','queued') : null);
  const {result}=setup(); act(() => events.handlers['import://progress']({payload:progress()}));
  await act(async () => events.handlers.drop({payload:{type:'drop',paths:['/a.stl']}}));
  expect(invoke).toHaveBeenCalledWith('start_dropped_import',{paths:['/a.stl']}); expect(result.current.queued).toBe(1);
  vi.mocked(invoke).mockClear();
  renderHook(() => useFileImport({enabled:false,catalogBaseDir:null,activeFolderId:'all',onImported:vi.fn(),refreshFiles:vi.fn(),refreshFolders:vi.fn()}));
  await act(async () => events.handlers.drop({payload:{type:'drop',paths:['/b.stl']}}));
  expect(invoke).not.toHaveBeenCalledWith('start_dropped_import',expect.anything());
});
it('counts all waiting jobs and keeps the previous terminal result available', async () => {
  const {result}=setup(); await act(async () => events.handlers['import://finished']({payload:finished()}));
  act(() => { events.handlers['import://progress']({payload:progress('j2','queued')});events.handlers['import://progress']({payload:progress('j3','queued')}); });
  expect(result.current.queued).toBe(2); expect(result.current.result?.jobId).toBe('j1'); expect(result.current.jobId).toBe('j2'); expect(result.current.jobActive).toBe(true);
});
it('remembers an undismissed terminal job, and dismiss removes the stored ID', async () => {
  const {result}=setup(); await act(async () => events.handlers['import://finished']({payload:finished()}));
  expect(JSON.parse(sessionStorage.getItem('catalog-import-jobs')!)).toEqual([{jobId:'j1',source:'files'}]);
  act(() => result.current.dismiss()); expect(JSON.parse(sessionStorage.getItem('catalog-import-jobs')!)).toEqual([]);
});
