import type { ModelFile, ModelFileSummary, Folder } from '../types';

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

// Projiziert einen ModelFile-Testfixture auf die schlanke
// list_file_summaries-Form (Finding M-01), damit Store-Tests nicht zwei
// unabhaengige Fixture-Definitionen pflegen muessen.
export function makeModelFileSummary(
  overrides: Partial<ModelFile> & { hasRenderSnapshot?: boolean } = {},
): ModelFileSummary {
  const m = makeModelFile(overrides);
  return {
    id: m.id,
    name: m.name,
    path: m.path,
    folderId: m.folderId,
    fileType: '3mf',
    fileSizeBytes: m.fileSizeBytes,
    dimensionsMm: m.dimensionsMm,
    volumeCm3: m.volumeCm3,
    objectCount: m.objectCount,
    importedAt: m.importedAt,
    printStatus: m.printStatus,
    favorite: m.favorite,
    queuePosition: m.queuePosition,
    thumbnailImage: m.thumbnailImage,
    // Bugfix (2026-09-20): list_file_summaries liefert render_snapshot_png
    // jetzt mit, spiegelt hier also den zugrundeliegenden ModelFile-Wert
    // wider statt hartcodiert null zu sein.
    renderSnapshotImage: m.renderSnapshotImage,
    // Standard: leitet sich aus demselben Wert ab - kann bei Bedarf explizit
    // ueberschrieben werden, um eine Abweichung zwischen Blob und Flag
    // nachzubilden (z.B. fuer Regressionstests des frueheren Finding-1-Bugs).
    hasRenderSnapshot: overrides.hasRenderSnapshot ?? m.renderSnapshotImage !== null,
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
