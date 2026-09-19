import { describe, expect, it } from 'vitest';
import { buildGroupedFolderTree } from './groupedFolderTree';
import { makeModelFile, makeFolder } from '../test/factories';

describe('buildGroupedFolderTree', () => {
  it('buckets files under their direct folder', () => {
    const folders = [makeFolder({ id: 'a', parentId: null })];
    const models = [makeModelFile({ id: '1', folderId: 'a' }), makeModelFile({ id: '2', folderId: '' })];
    const { roots, noFolder } = buildGroupedFolderTree(folders, models);
    expect(roots).toHaveLength(1);
    expect(roots[0].files.map((f) => f.id)).toEqual(['1']);
    expect(noFolder.map((f) => f.id)).toEqual(['2']);
  });

  it('nests subfolders under their parent as children', () => {
    const folders = [
      makeFolder({ id: 'a', parentId: null }),
      makeFolder({ id: 'b', parentId: 'a' }),
    ];
    const models = [makeModelFile({ id: '1', folderId: 'a' }), makeModelFile({ id: '2', folderId: 'b' })];
    const { roots } = buildGroupedFolderTree(folders, models);
    expect(roots).toHaveLength(1);
    expect(roots[0].children).toHaveLength(1);
    expect(roots[0].children[0].folder.id).toBe('b');
    expect(roots[0].children[0].files.map((f) => f.id)).toEqual(['2']);
  });

  it('computes totalCount recursively including all descendant files', () => {
    const folders = [
      makeFolder({ id: 'a', parentId: null }),
      makeFolder({ id: 'b', parentId: 'a' }),
      makeFolder({ id: 'c', parentId: 'b' }),
    ];
    const models = [
      makeModelFile({ id: '1', folderId: 'a' }),
      makeModelFile({ id: '2', folderId: 'b' }),
      makeModelFile({ id: '3', folderId: 'c' }),
      makeModelFile({ id: '4', folderId: 'c' }),
    ];
    const { roots } = buildGroupedFolderTree(folders, models);
    expect(roots[0].totalCount).toBe(4);
    expect(roots[0].children[0].totalCount).toBe(3);
    expect(roots[0].children[0].children[0].totalCount).toBe(2);
  });

  it('handles multiple independent root folders', () => {
    const folders = [makeFolder({ id: 'a', parentId: null }), makeFolder({ id: 'b', parentId: null })];
    const models = [makeModelFile({ id: '1', folderId: 'a' }), makeModelFile({ id: '2', folderId: 'b' })];
    const { roots } = buildGroupedFolderTree(folders, models);
    expect(roots.map((r) => r.folder.id).sort()).toEqual(['a', 'b']);
  });

  it('treats a folderId with no matching folder as "no folder"', () => {
    const folders = [makeFolder({ id: 'a', parentId: null })];
    const models = [makeModelFile({ id: '1', folderId: 'does-not-exist' })];
    const { roots, noFolder } = buildGroupedFolderTree(folders, models);
    expect(roots[0].files).toEqual([]);
    expect(noFolder.map((f) => f.id)).toEqual(['1']);
  });
});
