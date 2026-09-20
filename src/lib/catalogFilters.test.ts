import { describe, expect, it } from 'vitest';
import { filterAndSortModels, selectQueuedModels } from './catalogFilters';
import { makeModelFile, makeFolder } from '../test/factories';

describe('filterAndSortModels', () => {
  it('filters by folder including descendants', () => {
    const folders = [makeFolder({ id: 'a', parentId: null }), makeFolder({ id: 'b', parentId: 'a' })];
    const models = [
      makeModelFile({ id: '1', folderId: 'a' }),
      makeModelFile({ id: '2', folderId: 'b' }),
      makeModelFile({ id: '3', folderId: 'unrelated' }),
    ];
    const result = filterAndSortModels(models, folders, {
      activeFolderId: 'a', activeTag: null, activeCreator: null, query: '', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1', '2']);
  });

  it('filters by tag', () => {
    const models = [makeModelFile({ id: '1', tags: ['red'] }), makeModelFile({ id: '2', tags: [] })];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: 'red', activeCreator: null, query: '', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('filters by case-insensitive query', () => {
    const models = [makeModelFile({ id: '1', name: 'Benchy' }), makeModelFile({ id: '2', name: 'Vase' })];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, activeCreator: null, query: 'BENCH', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('query also matches a tag, not just the name', () => {
    const models = [
      makeModelFile({ id: '1', name: 'Adapter', tags: ['bambu', 'mount'] }),
      makeModelFile({ id: '2', name: 'Vase', tags: ['decor'] }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, activeCreator: null, query: 'bambu', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('query also matches the creator', () => {
    const models = [
      makeModelFile({ id: '1', name: 'Adapter', creator: 'CarlFromUp' }),
      makeModelFile({ id: '2', name: 'Vase', creator: null }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, activeCreator: null, query: 'carlfromup', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('query also matches the file path', () => {
    const models = [
      makeModelFile({ id: '1', name: 'Adapter', path: '/mnt/Daten2/3D Druck Sammelordner/adapter.3mf' }),
      makeModelFile({ id: '2', name: 'Vase', path: '/mnt/Daten2/vase.3mf' }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, activeCreator: null, query: 'sammelordner', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('sorts by size ascending', () => {
    const models = [
      makeModelFile({ id: '1', fileSizeBytes: 500 }),
      makeModelFile({ id: '2', fileSizeBytes: 100 }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, activeCreator: null, query: '', sort: 'size',
    });
    expect(result.map((m) => m.id)).toEqual(['2', '1']);
  });

  it('sorts by date descending (most recent first)', () => {
    const models = [
      makeModelFile({ id: '1', importedAt: '2026-01-01' }),
      makeModelFile({ id: '2', importedAt: '2026-06-01' }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, activeCreator: null, query: '', sort: 'date',
    });
    expect(result.map((m) => m.id)).toEqual(['2', '1']);
  });
});

describe('selectQueuedModels', () => {
  it('returns only queued models, ordered by position', () => {
    const models = [
      makeModelFile({ id: '1', queuePosition: 2 }),
      makeModelFile({ id: '2', queuePosition: null }),
      makeModelFile({ id: '3', queuePosition: 1 }),
    ];
    const result = selectQueuedModels(models);
    expect(result.map((m) => m.id)).toEqual(['3', '1']);
  });
});
