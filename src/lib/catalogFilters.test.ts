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
      activeFolderId: 'a', activeTag: null, query: '', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1', '2']);
  });

  it('filters by tag', () => {
    const models = [makeModelFile({ id: '1', tags: ['red'] }), makeModelFile({ id: '2', tags: [] })];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: 'red', query: '', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('filters by case-insensitive query', () => {
    const models = [makeModelFile({ id: '1', name: 'Benchy' }), makeModelFile({ id: '2', name: 'Vase' })];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, query: 'BENCH', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('query also matches a tag, not just the name', () => {
    const models = [
      makeModelFile({ id: '1', name: 'Adapter', tags: ['bambu', 'mount'] }),
      makeModelFile({ id: '2', name: 'Vase', tags: ['decor'] }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, query: 'bambu', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('query also matches the creator', () => {
    const models = [
      makeModelFile({ id: '1', name: 'Adapter', creator: 'CarlFromUp' }),
      makeModelFile({ id: '2', name: 'Vase', creator: null }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, query: 'carlfromup', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('query also matches the file path', () => {
    const models = [
      makeModelFile({ id: '1', name: 'Adapter', path: '/mnt/Daten2/3D Druck Sammelordner/adapter.3mf' }),
      makeModelFile({ id: '2', name: 'Vase', path: '/mnt/Daten2/vase.3mf' }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, query: 'sammelordner', sort: 'name',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('query matches the translated label of an auto tag', () => {
    const models = [
      makeModelFile({ id: '1', name: 'Board', tags: ['mehrteilig'] }),
      makeModelFile({ id: '2', name: 'Vase', tags: ['decor'] }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, query: 'multipart', sort: 'name', language: 'en',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('query still matches the canonical auto tag name', () => {
    const models = [makeModelFile({ id: '1', name: 'Board', tags: ['mehrteilig'] })];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, query: 'mehrteilig', sort: 'name', language: 'en',
    });
    expect(result.map((m) => m.id)).toEqual(['1']);
  });

  it('sorts by size ascending', () => {
    const models = [
      makeModelFile({ id: '1', fileSizeBytes: 500 }),
      makeModelFile({ id: '2', fileSizeBytes: 100 }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, query: '', sort: 'size', sortDirection: 'asc',
    });
    expect(result.map((m) => m.id)).toEqual(['2', '1']);
  });

  it('sorts by date descending (most recent first)', () => {
    const models = [
      makeModelFile({ id: '1', importedAt: '2026-01-01' }),
      makeModelFile({ id: '2', importedAt: '2026-06-01' }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, query: '', sort: 'imported',
    });
    expect(result.map((m) => m.id)).toEqual(['2', '1']);
  });

  it('applies a tool view before the other filters and keeps its order', () => {
    const now = new Date('2026-09-24T12:00:00.000Z');
    const models = [
      makeModelFile({ id: 'a', name: 'A', lastViewedAt: '2026-09-20T00:00:00.000Z', tags: ['deko'] }),
      makeModelFile({ id: 'b', name: 'B', lastViewedAt: '2026-09-23T00:00:00.000Z', tags: ['deko'] }),
      makeModelFile({ id: 'c', name: 'C', lastViewedAt: null, tags: ['deko'] }),
      makeModelFile({ id: 'd', name: 'D', lastViewedAt: '2026-09-24T00:00:00.000Z', tags: [] }),
    ];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: 'deko', query: '', sort: 'name',
      toolView: 'recent', now,
    });
    expect(result.map((m) => m.id)).toEqual(['b', 'a']);
  });

  it('sorts normally when no tool view is active', () => {
    const models = [makeModelFile({ id: '2', name: 'Zebra' }), makeModelFile({ id: '1', name: 'Adler' })];
    const result = filterAndSortModels(models, [], {
      activeFolderId: 'all', activeTag: null, query: '', sort: 'name', toolView: null,
    });
    expect(result.map((m) => m.id)).toEqual(['1', '2']);
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

describe('sort directions', () => {
  const criteria = { activeFolderId: 'all', activeTag: null, query: '' };
  it.each(['name', 'imported', 'modified', 'size', 'vol', 'viewed'] as const)('sorts %s both ways', sort => {
    const models = [
      makeModelFile({ id: 'high', name: 'Z', importedAt: '2026-02-01', fileModifiedAt: '2026-02-01', lastViewedAt: '2026-02-01', fileSizeBytes: 20, volumeCm3: 20 }),
      makeModelFile({ id: 'low', name: 'A', importedAt: '2026-01-01', fileModifiedAt: '2026-01-01', lastViewedAt: '2026-01-01', fileSizeBytes: 10, volumeCm3: 10 }),
    ];
    expect(filterAndSortModels(models, [], { ...criteria, sort, sortDirection: 'asc' }).map(m => m.id)).toEqual(['low', 'high']);
    expect(filterAndSortModels(models, [], { ...criteria, sort, sortDirection: 'desc' }).map(m => m.id)).toEqual(['high', 'low']);
    expect(models.map(m => m.id)).toEqual(['high', 'low']);
  });
  it.each(['modified', 'viewed', 'vol'] as const)('keeps missing %s last and ties alphabetical both ways', sort => {
    const models = [
      makeModelFile({ id: 'missingZ', name: 'Z' }),
      makeModelFile({ id: 'knownZ', name: 'Z', fileModifiedAt: '2026-01-01', lastViewedAt: '2026-01-01', volumeCm3: 0 }),
      makeModelFile({ id: 'missingA', name: 'A' }),
      makeModelFile({ id: 'knownA', name: 'A', fileModifiedAt: '2026-01-01', lastViewedAt: '2026-01-01', volumeCm3: 0 }),
    ];
    for (const sortDirection of ['asc', 'desc'] as const) {
      expect(filterAndSortModels(models, [], { ...criteria, sort, sortDirection }).map(m => m.id)).toEqual(['knownA', 'knownZ', 'missingA', 'missingZ']);
    }
  });
  it.each(['imported', 'size'] as const)('breaks %s ties by name both ways', sort => {
    const models = [makeModelFile({ name: 'Z' }), makeModelFile({ name: 'A' })];
    for (const sortDirection of ['asc', 'desc'] as const) {
      expect(filterAndSortModels(models, [], { ...criteria, sort, sortDirection }).map(m => m.name)).toEqual(['A', 'Z']);
    }
  });
});

it('sorts modified dates independently of import dates', () => {
  const models = [
    makeModelFile({ id: 'old', importedAt: '2026-02-01', fileModifiedAt: '2026-01-01' }),
    makeModelFile({ id: 'new', importedAt: '2026-01-01', fileModifiedAt: '2026-02-01' }),
  ];
  expect(filterAndSortModels(models, [], { activeFolderId: 'all', activeTag: null, query: '', sort: 'modified', sortDirection: 'desc' }).map(m => m.id)).toEqual(['new', 'old']);
});

it.each(['asc', 'desc'] as const)('ignores the selected sort and %s direction in tool views', sortDirection => {
  const models = [
    makeModelFile({ id: 'old', name: 'A', volumeCm3: 20, lastViewedAt: '2026-01-01' }),
    makeModelFile({ id: 'new', name: 'Z', volumeCm3: 10, lastViewedAt: '2026-02-01' }),
  ];
  expect(filterAndSortModels(models, [], { activeFolderId: 'all', activeTag: null, query: '', sort: 'vol', sortDirection, toolView: 'recent' }).map(m => m.id)).toEqual(['new', 'old']);
});
