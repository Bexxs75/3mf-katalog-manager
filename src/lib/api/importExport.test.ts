import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import * as importExportApi from './importExport';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('importExport api', () => {
  it('importFiles', async () => {
    vi.mocked(invoke).mockResolvedValue({ imported: [], duplicateCount: 0 });
    await importExportApi.importFiles();
    expect(invoke).toHaveBeenCalledWith('import_files');
  });
  it('importFolder', async () => {
    vi.mocked(invoke).mockResolvedValue({ imported: [], duplicateCount: 0 });
    await importExportApi.importFolder();
    expect(invoke).toHaveBeenCalledWith('import_folder');
  });
  it('importDropped', async () => {
    vi.mocked(invoke).mockResolvedValue({ imported: [], duplicateCount: 0 });
    await importExportApi.importDropped(['/a.3mf']);
    expect(invoke).toHaveBeenCalledWith('import_dropped', { paths: ['/a.3mf'] });
  });
  it('exportCatalog', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await importExportApi.exportCatalog('{}');
    expect(invoke).toHaveBeenCalledWith('export_catalog', { settingsJson: '{}' });
  });
  it('importCatalog', async () => {
    vi.mocked(invoke).mockResolvedValue({ imported: true, settingsJson: null });
    const result = await importExportApi.importCatalog();
    expect(result.imported).toBe(true);
    expect(invoke).toHaveBeenCalledWith('import_catalog');
  });
});

it('starts a job with its captured target and retrieves progress/result', async () => {
  vi.mocked(invoke).mockResolvedValue({ jobId: 'job-1' });
  await importExportApi.startImport('files', 'folder-7');
  expect(invoke).toHaveBeenCalledWith('start_import', { source: 'files', targetFolderId: 'folder-7' });
  await importExportApi.getImportResult('job-1');
  expect(invoke).toHaveBeenCalledWith('get_import_result', { jobId: 'job-1' });
  await importExportApi.getImportState('job-1');
  expect(invoke).toHaveBeenCalledWith('get_import_state', { jobId: 'job-1' });
  await importExportApi.cancelImport('job-1');
  expect(invoke).toHaveBeenCalledWith('cancel_import', { jobId: 'job-1' });
});
it('keeps separate proof-bearing entrypoints for drops and adoption', async () => {
  await importExportApi.startDroppedImport(['/model.stl']);
  expect(invoke).toHaveBeenCalledWith('start_dropped_import', { paths: ['/model.stl'] });
  await importExportApi.startAdoptImport('/picked');
  expect(invoke).toHaveBeenCalledWith('start_adopt_import', { path: '/picked' });
});

it('forwards typed progress and terminal payloads through the new event names', async () => {
  const progress = vi.fn(); const finished = vi.fn();
  await importExportApi.onImportProgress(progress);
  await importExportApi.onImportFinished(finished);
  expect(listen).toHaveBeenCalledWith('import://progress', expect.any(Function));
  expect(listen).toHaveBeenCalledWith('import://finished', expect.any(Function));
  const progressHandler = vi.mocked(listen).mock.calls.find(([name]) => name === 'import://progress')![1];
  const finishedHandler = vi.mocked(listen).mock.calls.find(([name]) => name === 'import://finished')![1];
  progressHandler({ event: 'import://progress', id: 1, payload: { jobId: 'j', inFlight: 2 } });
  finishedHandler({ event: 'import://finished', id: 2, payload: { jobId: 'j', state: 'cancelled' } });
  expect(progress).toHaveBeenCalledWith({ jobId: 'j', inFlight: 2 });
  expect(finished).toHaveBeenCalledWith({ jobId: 'j', state: 'cancelled' });
});
