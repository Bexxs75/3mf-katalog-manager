import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as importExportApi from './importExport';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
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
