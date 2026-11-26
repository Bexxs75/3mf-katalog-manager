import { describe, expect, it } from 'vitest';
import type { DiskReport } from '../lib/api/diskSync';
import { groupMissingFolders } from './DiskReportModel';
import { groupMissingByFolder } from './CatalogCleanupDialog';
import { makeModelFile } from '../test/factories';

const base: DiskReport = {
  roots: [{ name: 'a', status: 'ok', mostlyMissing: false }],
  missingModels: [
    { id: '1', root: 0, relativePath: 'Alt/Sub/x.3mf' }, { id: '2', root: 0, relativePath: 'Alt/Sub/y.3mf' },
    { id: '3', root: 0, relativePath: 'Alt/z.3mf' }, { id: '4', root: 0, relativePath: 'Other/q.3mf' }, { id: '5', root: 0, relativePath: 'Other/r.3mf' },
  ],
  newFiles: [], danglingCompanions: [], modelsOutsideRoots: 0, companionsOutsideRoots: 0, scannedEntries: 0, truncated: false, truncatedBy: [],
};

describe('groupMissingFolders (sync dialog)', () => {
  it('falls back to the parent heuristic when the server sends no missingFolders', () => {
    const { folders, single } = groupMissingFolders(base);
    expect(folders.map(f => f.path).sort()).toEqual(['Alt/Sub', 'Other']);
    expect(single.map(m => m.id)).toEqual(['3']);
  });
  it('prefers the verified folders of the server over the heuristic', () => {
    const { folders, single } = groupMissingFolders({ ...base, missingFolders: [{ relativePath: 'Alt', root: 0, models: ['1', '2', '3'] }] });
    expect(folders).toHaveLength(1);
    expect(folders[0]).toMatchObject({ path: 'Alt', root: 0 });
    expect(folders[0].models.map(m => m.id)).toEqual(['1', '2', '3']);
    // 'Other' is not verified, so its models stay plain rows
    expect(single.map(m => m.id)).toEqual(['4', '5']);
  });
  it('web, empty missingFolders: no folder group despite two models in the same folder', () => {
    const { folders, single } = groupMissingFolders({ ...base, missingFolders: [] });
    expect(folders).toHaveLength(0);
    expect(single.map(m => m.id)).toEqual(['1', '2', '3', '4', '5']);
  });
  it('keeps a single-model folder and unknown ids as plain rows', () => {
    const { folders, single } = groupMissingFolders({ ...base, missingFolders: [{ relativePath: 'Other', root: 0, models: ['4', 'gone'] }] });
    expect(folders).toHaveLength(0);
    expect(single).toHaveLength(5);
  });
});

describe('groupMissingByFolder (cleanup dialog)', () => {
  const gone = (id: string, path: string) => makeModelFile({ id, name: `m${id}.3mf`, path });
  const orphaned = [gone('1', '/r/Alt/Sub/a.3mf'), gone('2', '/r/Alt/Sub/b.3mf'), gone('3', '/r/Alt/c.3mf'), gone('4', '/r/Other/d.3mf'), gone('5', '/r/Other/e.3mf')];
  it('uses the heuristic without the server field', () => {
    const { folders, single } = groupMissingByFolder(orphaned);
    expect(folders.map(f => f.path).sort()).toEqual(['/r/Alt/Sub', '/r/Other']);
    expect(single.map(m => m.id)).toEqual(['3']);
  });
  it('web, empty missingFolders: no folder group despite two models in the same folder', () => {
    const { folders, single } = groupMissingByFolder(orphaned, []);
    expect(folders).toHaveLength(0);
    expect(single.map(m => m.id)).toEqual(['1', '2', '3', '4', '5']);
  });
  it('lets the server folders win', () => {
    const { folders, single } = groupMissingByFolder(orphaned, [{ relativePath: 'Alt', root: 0, models: ['1', '2', '3'] }]);
    expect(folders).toHaveLength(1);
    expect(folders[0].path).toBe('Alt');
    expect(folders[0].models.map(m => m.id)).toEqual(['1', '2', '3']);
    expect(single.map(m => m.id)).toEqual(['4', '5']);
  });
});
