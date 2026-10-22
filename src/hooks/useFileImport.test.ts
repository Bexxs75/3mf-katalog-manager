import { createElement } from 'react';
import { ImportSummaryBanner } from '../components/ImportSummaryBanner';
import { LanguageProvider } from '../i18n/LanguageContext';
import { importResult } from '../test/importFixtures';
import { beforeEach, expect, it, vi } from 'vitest';
import { act, render, screen, renderHook, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useFileImport } from './useFileImport';
import type { ImportJobResult, ImportProgress } from '../types';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const events = vi.hoisted(() => ({ handlers: {} as Record<string, (event: {payload: unknown}) => void>, unlisten: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn((name, cb) => { events.handlers[name] = cb; return Promise.resolve(events.unlisten); }) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: (cb: (e: {payload: unknown}) => void) => { events.handlers.drop = cb; return Promise.resolve(events.unlisten); } }) }));
export const counts = { imported: 1, importedNotPlaced: 0, duplicate: 0, skipped: 0, archive: 0, known: 1 };
const progress = (jobId = 'j1', state: ImportProgress['state'] = 'importing'): ImportProgress => ({ jobId, state, counts, scanComplete: true, archivesSkipped: 0, done: 1, total: 2, found: 2, inFlight: 1, current: 'a.stl', elapsedMs: 250 });
const finished = (state: ImportJobResult['state'] = 'finished'): ImportJobResult => ({ jobId: 'j1', source: 'files', state, parentJobId: null, jobError: null, scanComplete: true, archivesSkipped: 0, placementRequired: true, counts, groups: { imported: [], importedNotPlaced: [], duplicate: [], skipped: [], archive: [] } });
beforeEach(() => {
  sessionStorage.clear(); vi.clearAllMocks(); events.handlers = {};
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'start_import' ? { jobId: 'j1' } : null);
});
function setup(folder = 'first') {
  const refreshTags = vi.fn(), refreshFiles = vi.fn(), refreshFolders = vi.fn(), onImported = vi.fn();
  const hook = renderHook(({ folder }) => useFileImport({ enabled: true, catalogBaseDir: '/base', activeFolderId: folder, onImported, refreshTags, refreshFiles, refreshFolders }), { initialProps: { folder } });
  return { ...hook, refreshTags, refreshFiles, refreshFolders };
}
it('starts a job with the captured target and updates from progress', async () => {
  const { result, rerender } = setup();
  await act(async () => { await result.current.importFiles(); });
  rerender({ folder: 'second' });
  expect(invoke).toHaveBeenCalledWith('start_import', { source: 'files', targetFolderId: 'first' });
  act(() => events.handlers['import://progress']({ payload: progress() }));
  expect(result.current.progress?.current).toBe('a.stl'); expect(result.current.jobActive).toBe(true);
});
it.each((['files', 'folder', 'folderContents', 'dropped', 'setupAdopt', 'archive'] as const).flatMap(source =>
  (['finished', 'cancelled', 'failed'] as const).map(state => ({ source, state })),
))('$source / $state refreshes files, folders and tags once', async ({ source, state }) => {
  const { result, refreshFiles, refreshFolders, refreshTags } = setup();
  await act(async () => events.handlers['import://finished']({payload: {...finished(state), source}}));
  expect(result.current.result?.state).toBe(state); expect(refreshFiles).toHaveBeenCalledOnce(); expect(refreshFolders).toHaveBeenCalledOnce(); expect(refreshTags).toHaveBeenCalledOnce();
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
  const args = {enabled:true,catalogBaseDir:'/base',activeFolderId:'all',rootFolderId:'root',onImported:vi.fn(),refreshFiles:vi.fn(),refreshFolders:vi.fn(),refreshTags:vi.fn()};
  const {result} = renderHook(() => useFileImport(args));
  await act(async () => result.current.importFiles());
  expect(invoke).toHaveBeenCalledWith('start_import',{source:'files',targetFolderId:'root'});
});
it('starts files without a root and folder imports without a placement target', async () => {
  vi.mocked(invoke).mockResolvedValue({jobId:null});
  const {result} = renderHook(() => useFileImport({enabled:true,catalogBaseDir:null,activeFolderId:'all',onImported:vi.fn(),refreshFiles:vi.fn(),refreshFolders:vi.fn(),refreshTags:vi.fn()}));
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
  const {result,refreshFiles,refreshTags}=setup();
  await act(async () => {events.handlers['import://finished']({payload:finished()});events.handlers['import://finished']({payload:finished()});});
  expect(refreshFiles).toHaveBeenCalledOnce(); expect(refreshTags).toHaveBeenCalledOnce(); act(() => result.current.dismiss()); expect(result.current.progress).toBeNull(); expect(result.current.result).toBeNull();
});
it('accepts dropped paths during another import and ignores drops in other views', async () => {
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'start_dropped_import' ? {jobId:'j2'} : cmd === 'get_import_state' ? progress('j2','queued') : null);
  const {result}=setup(); act(() => events.handlers['import://progress']({payload:progress()}));
  await act(async () => events.handlers.drop({payload:{type:'drop',paths:['/a.stl']}}));
  expect(invoke).toHaveBeenCalledWith('start_dropped_import',{paths:['/a.stl']}); expect(result.current.queued).toBe(1);
  vi.mocked(invoke).mockClear();
  renderHook(() => useFileImport({enabled:false,catalogBaseDir:null,activeFolderId:'all',onImported:vi.fn(),refreshFiles:vi.fn(),refreshFolders:vi.fn(),refreshTags:vi.fn()}));
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

it('imports folder contents into the captured active folder', async () => {
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'start_import' ? { jobId: 'j1' } : cmd === 'get_import_state' ? progress() : null);
  const { result, rerender } = setup();
  await act(async () => result.current.importFolderContents());
  rerender({ folder: 'second' });
  expect(invoke).toHaveBeenCalledWith('start_import', { source: 'folderContents', targetFolderId: 'first' });
  expect(result.current.meta?.source).toBe('folderContents');
});

it('imports folder contents into the catalog root from all models', async () => {
  const { result } = renderHook(() => useFileImport({ enabled: true, catalogBaseDir: '/base', activeFolderId: 'all', rootFolderId: 'root', onImported: vi.fn(), refreshFiles: vi.fn(), refreshFolders: vi.fn(), refreshTags: vi.fn() }));
  await act(async () => result.current.importFolderContents());
  expect(invoke).toHaveBeenCalledWith('start_import', { source: 'folderContents', targetFolderId: 'root' });
});

function ImportBannerHarness() {
  const fileImport = useFileImport({enabled: true, catalogBaseDir: '/base', activeFolderId: 'all', onImported: vi.fn(), refreshFiles: vi.fn(), refreshFolders: vi.fn(), refreshTags: vi.fn()});
  return fileImport.importBanner && createElement(ImportSummaryBanner, {...fileImport.importBanner, onClose: fileImport.dismissImportBanner});
}

it.each([0, 2])('renders the archive job result in the banner with %s other files', async otherFilesExtracted => {
  localStorage.setItem('3mf-katalog-language', 'de');
  render(createElement(LanguageProvider, null, createElement(ImportBannerHarness)));
  const archive = importResult.groups.archive[1];
  if (archive.state === 'pending') throw new Error('expected completed fixture');
  const value: ImportJobResult = {...finished(), source: 'archive', groups: {...finished().groups, archive: [{...archive, otherFilesExtracted, unsafeSkipped: 0, models: {imported: [], skipped: [], duplicates: []}}]}};
  await act(async () => events.handlers['import://finished']({payload: value}));
  if (otherFilesExtracted) expect(screen.getByText(/2 weitere Dateien wurden entpackt/)).toBeVisible();
  else expect(screen.queryByText(/weitere Dateien wurden entpackt/)).not.toBeInTheDocument();
});

it('shows companion counts from an archive job even when no other files are left', async () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  render(createElement(LanguageProvider, null, createElement(ImportBannerHarness)));
  const archive = importResult.groups.archive[1];
  if (archive.state === 'pending') throw new Error('expected completed fixture');
  const value: ImportJobResult = {...finished(), source: 'archive', groups: {...finished().groups, archive: [{...archive, otherFilesExtracted: 1, companionsAssigned: 3, companionsIgnored: 1, unsafeSkipped: 0, models: {imported: [], skipped: [], duplicates: []}}]}};
  await act(async () => events.handlers['import://finished']({payload: value}));
  expect(screen.getByText('3 Begleitdateien wurden Modellen zugeordnet.')).toBeVisible();
  expect(screen.getByText(/1 Datei ignoriert/)).toBeVisible();
});

it.each([false, true])('keeps an archive hint for 15 seconds, or until dismissed with problems (%s)', jobHasProblems => {
  vi.useFakeTimers();
  try {
    localStorage.setItem('3mf-katalog-language', 'de');
    render(createElement(LanguageProvider, null, createElement(ImportBannerHarness)));
    const archive = importResult.groups.archive[1];
    if (archive.state === 'pending') throw new Error('expected completed fixture');
    const clean = {...archive, unsafeSkipped: 0, models: {imported: [], skipped: [], duplicates: []}};
    const value: ImportJobResult = {...finished(), source: 'archive', groups: {...finished().groups, archive: [
      {...clean, otherFilesExtracted: 1, archiveDeleted: true, deleteError: jobHasProblems ? 'delete denied' : null},
      {...clean, path: '/second.zip', otherFilesExtracted: 2},
    ]}};
    act(() => events.handlers['import://finished']({payload: value}));
    expect(screen.getByText(/3 weitere Dateien wurden entpackt/)).toBeVisible();
    expect(screen.getAllByRole('listitem')).toHaveLength(1);
    act(() => vi.advanceTimersByTime(14999));
    expect(screen.getByText(/3 weitere Dateien wurden entpackt/)).toBeVisible();
    act(() => vi.advanceTimersByTime(1));
    expect(!!screen.queryByText(/3 weitere Dateien wurden entpackt/)).toBe(jobHasProblems);
  } finally { vi.useRealTimers(); }
});

it('recovers an archive hint through the result query after start_archive_import', async () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  const archive = importResult.groups.archive[1];
  if (archive.state === 'pending') throw new Error('expected completed fixture');
  const value: ImportJobResult = {...finished(), jobId: 'archive-job', source: 'archive', parentJobId: 'j1', groups: {...finished().groups, archive: [{...archive, otherFilesExtracted: 2}]}};
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'inspect_archives' ? [{path: '/gut.zip', status: 'ok'}] : cmd === 'start_archive_import' ? {jobId: 'archive-job'} : cmd === 'get_import_state' ? progress('archive-job', 'finished') : cmd === 'get_import_result' ? value : null);
  const {result} = setup();
  await act(async () => events.handlers['import://finished']({payload: {...finished(), groups: {...finished().groups, archive: [{path: '/gut.zip', state: 'pending', grantId: 'g'}]}}}));
  await act(async () => result.current.startArchives('/target', [{path: '/gut.zip', folderName: 'gut', onConflict: 'new', expectedSize: 1, expectedModifiedUnixMs: 1}], false));
  expect(result.current.importBanner).not.toBeNull();
  render(createElement(LanguageProvider, null, createElement(ImportSummaryBanner, {...result.current.importBanner!, onClose: result.current.dismissImportBanner})));
  expect(screen.getByText(/2 weitere Dateien wurden entpackt/)).toBeVisible();
});

it('keeps the skipped-files banner when an archive job extracts no other files', async () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  const { result } = setup();
  act(() => result.current.mergeImported({ imported: [], duplicateCount: 0, skipped: [{path: '/empty.stl', reason: 'empty'}] }));
  const banner = () => createElement(LanguageProvider, null, result.current.importBanner && createElement(ImportSummaryBanner, {...result.current.importBanner, onClose: result.current.dismissImportBanner}));
  const view = render(banner());
  expect(screen.getByText(/empty.stl/)).toBeVisible();
  const archive = importResult.groups.archive[1];
  if (archive.state === 'pending') throw new Error('expected completed fixture');
  const value: ImportJobResult = {...finished(), source: 'archive', groups: {...finished().groups, archive: [{...archive, otherFilesExtracted: 0}]}};
  await act(async () => events.handlers['import://finished']({payload: value}));
  view.rerender(banner());
  expect(screen.getByText(/empty.stl/)).toBeVisible();
});

it.each(['failed', 'existingSkipped', 'blockedSkipped'] as const)('keeps the archive hint after 20 seconds for %s alone', problem => {
  vi.useFakeTimers();
  try {
    localStorage.setItem('3mf-katalog-language', 'de');
    render(createElement(LanguageProvider, null, createElement(ImportBannerHarness)));
    const archive = importResult.groups.archive[1];
    if (archive.state === 'pending') throw new Error('expected completed fixture');
    const value: ImportJobResult = {...finished(), source: 'archive', groups: {...finished().groups, archive: [{
      ...archive,
      state: problem === 'failed' ? 'failed' : 'finished',
      error: null,
      deleteError: null,
      existingSkipped: problem === 'existingSkipped' ? 1 : 0,
      blockedSkipped: problem === 'blockedSkipped' ? 1 : 0,
      unsafeSkipped: 0,
      otherFilesExtracted: 2,
      models: {imported: [], skipped: [], duplicates: []},
    }]}};
    act(() => events.handlers['import://finished']({payload: value}));
    expect(screen.getByText(/2 weitere Dateien wurden entpackt/)).toBeVisible();
    act(() => vi.advanceTimersByTime(20000));
    expect(screen.getByText(/2 weitere Dateien wurden entpackt/)).toBeVisible();
  } finally { vi.useRealTimers(); }
});
it('tracks a source folder import like any other job and hands start errors to the caller', async () => {
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'import_source_files' ? { jobId: 'j7' } : cmd === 'get_import_state' ? progress('j7') : null);
  const { result } = setup();
  await act(async () => { await result.current.importSourceFiles(['/dl/a.3mf']); });
  expect(invoke).toHaveBeenCalledWith('import_source_files', { paths: ['/dl/a.3mf'] });
  expect(result.current.jobId).toBe('j7'); expect(result.current.jobActive).toBe(true);
  vi.mocked(invoke).mockRejectedValueOnce({ message: 'Importlauf aktiv', expected: true });
  await expect(act(async () => { await result.current.importSourceFiles(['/dl/b.3mf']); })).rejects.toMatchObject({ message: 'Importlauf aktiv' });
  expect(result.current.error).toBeNull();
});
