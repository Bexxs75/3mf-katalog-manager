import type { ModelFile, Folder } from '../types';

export function makeModelFile(overrides: Partial<ModelFile> = {}): ModelFile {
  return {
    id: 'model-1',
    name: 'Test Model',
    path: '/catalog/model-1.3mf',
    folderId: 'root',
    tags: [],
    origin: 'local',
    sync: 'local-only',
    dimensionsMm: null,
    volumeCm3: null,
    objectCount: null,
    plateCount: null,
    materials: [],
    fileSizeBytes: 1024,
    importedAt: '2026-01-01T00:00:00.000Z',
    printStatus: 'not_printed',
    estimatedWeightG: null,
    weightSource: 'estimated',
    sliceInfo: null,
    costEstimate: null,
    lastViewedAt: null,
    creator: null,
    customImage: null,
    thumbnailImage: null,
    renderSnapshotImage: null,
    sourceUrl: null,
    queuePosition: null,
    favorite: false,
    deletedAt: null,
    ...overrides,
  };
}

export function makeFolder(overrides: Partial<Folder> = {}): Folder {
  return {
    id: 'folder-1',
    name: 'Test Folder',
    path: '/catalog/Test Folder',
    parentId: null,
    count: 0,
    ...overrides,
  };
}
