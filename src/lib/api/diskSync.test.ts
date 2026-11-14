import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { applyCatalogDiskSync, countKnownCopies, declinedCopies, listKnownCopies, resetKnownCopies, type DiskScan } from './diskSync';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); vi.mocked(invoke).mockResolvedValue(0); });

const copy = (name: string) => ({ path: `/m/${name}`, relativePath: name, root: '/m', id: null, duplicateOfName: 'Orig', duplicateOfPath: '/m/orig.3mf' });
const scan: DiskScan = {
  newFolders: [], newFiles: [], renamedFiles: [], missingFiles: [], missingFolders: [], unreachableRoots: [], skippedPaths: [],
  trashBlockedCount: 0, knownCopyCount: 0, duplicateFiles: [copy('a.3mf'), copy('b.3mf')],
};

describe('remembered copies api', () => {
  it('calls the three commands without arguments', async () => {
    await countKnownCopies(); await listKnownCopies(); await resetKnownCopies();
    expect(vi.mocked(invoke).mock.calls.map(c => c[0])).toEqual(['count_known_copies', 'list_known_copies', 'reset_known_copies']);
    expect(vi.mocked(invoke).mock.calls.every(c => c[1] === undefined)).toBe(true);
  });
  it('names the unticked copies as declined, never the ticked ones', () => {
    expect(declinedCopies(scan, { paths: new Set(['/m/a.3mf']), renameIds: new Set() })).toEqual(['/m/b.3mf']);
    expect(declinedCopies(scan, { paths: new Set(['/m/a.3mf', '/m/b.3mf']), renameIds: new Set() })).toEqual([]);
  });
  it('sends declinedCopies only when there are some', async () => {
    await applyCatalogDiskSync(scan, false, { paths: new Set(['/m/a.3mf']), renameIds: new Set() });
    expect(vi.mocked(invoke)).toHaveBeenLastCalledWith('apply_catalog_disk_sync', { selection: expect.objectContaining({ newPaths: ['/m/a.3mf'], declinedCopies: ['/m/b.3mf'] }) });
    await applyCatalogDiskSync({ ...scan, duplicateFiles: [] }, false, { paths: new Set(), renameIds: new Set() });
    expect(vi.mocked(invoke).mock.lastCall![1]).toEqual({ selection: { newPaths: [], missingIds: [], removeMissing: false, renames: [] } });
  });
});
