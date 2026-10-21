import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as sourcesApi from './sourceFolders';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('source folders api', () => {
  it('lists, adds and removes source folders', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await sourcesApi.listSourceFolders();
    await sourcesApi.addSourceFolder('/dl');
    await sourcesApi.removeSourceFolder('2');
    expect(invoke).toHaveBeenCalledWith('list_source_folders');
    expect(invoke).toHaveBeenCalledWith('add_source_folder', { path: '/dl' });
    expect(invoke).toHaveBeenCalledWith('remove_source_folder', { id: '2' });
  });
  it('reads and writes the start check switch', async () => {
    vi.mocked(invoke).mockResolvedValue(false);
    await sourcesApi.getSourceCheckOnStart();
    await sourcesApi.setSourceCheckOnStart(true);
    expect(invoke).toHaveBeenCalledWith('get_source_check_on_start');
    expect(invoke).toHaveBeenCalledWith('set_source_check_on_start', { enabled: true });
  });
  it('scans, imports and declines files', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await sourcesApi.scanSourceFolders();
    await sourcesApi.importSourceFiles(['/dl/a.stl']);
    await sourcesApi.rejectSourceFiles(['/dl/b.stl']);
    expect(invoke).toHaveBeenCalledWith('scan_source_folders');
    expect(invoke).toHaveBeenCalledWith('import_source_files', { paths: ['/dl/a.stl'] });
    expect(invoke).toHaveBeenCalledWith('reject_source_files', { paths: ['/dl/b.stl'] });
  });
  it('counts and resets declined files', async () => {
    vi.mocked(invoke).mockResolvedValue(0);
    await sourcesApi.listRejectedCount();
    await sourcesApi.resetRejectedSourceFiles();
    expect(invoke).toHaveBeenCalledWith('list_rejected_count');
    expect(invoke).toHaveBeenCalledWith('reset_rejected_source_files');
  });
});
