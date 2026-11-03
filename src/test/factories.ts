import type { ModelFile, ModelFileSummary, Folder, Printer, PrintInfo, PrintFacts } from '../types';

export function makePrintInfo(overrides: Partial<PrintInfo> = {}): PrintInfo {
  return {
    sliceState: 'sliced',
    settingsState: 'read',
    readerCurrent: true,
    readAt: '2026-11-01T00:00:00Z',
    printerModel: null,
    bedType: null,
    layerHeightMm: null,
    nozzleMm: { value: null, mixed: false },
    bedTempC: { value: null, mixed: false },
    nozzleTempC: { value: null, mixed: false },
    plates: [],
    filamentColors: [],
    ...overrides,
  };
}

/** Print facts of a list row; a bare `plateCount: 0` is what a file with nothing but the state yields. */
export function makePrintFacts(overrides: Partial<PrintFacts> = {}): PrintFacts {
  return { plateCount: 0, ...overrides };
}

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
    fileModifiedAt: null,
    printStatus: 'not_printed',
    estimatedWeightG: null,
    weightSource: 'estimated',
    sliceInfo: null,
    printInfo: null,
    costEstimate: null,
    lastViewedAt: null,
    contentHash: null,
    creator: null,
    customImage: null,
    thumbnailImage: null,
    renderSnapshotImage: null,
    sourceUrl: null,
    license: null,
    sourcePlatform: null,
    sourceModelId: null,
    sourceProfileId: null,
    queuePosition: null,
    favorite: false,
    deletedAt: null,
    ...overrides,
  };
}

// Projects a ModelFile fixture onto the summary shape (one fixture definition for both).
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
    materials: m.materials,
    estimatedWeightG: m.estimatedWeightG,
    weightSource: m.weightSource,
    fileSizeBytes: m.fileSizeBytes,
    dimensionsMm: m.dimensionsMm,
    volumeCm3: m.volumeCm3,
    objectCount: m.objectCount,
    importedAt: m.importedAt,
    fileModifiedAt: m.fileModifiedAt,
    printStatus: m.printStatus,
    favorite: m.favorite,
    queuePosition: m.queuePosition,
    hasThumbnail: overrides.hasThumbnail ?? m.thumbnailImage !== null,
    // Default derived from the blob; can be overridden to let blob and flag
    // diverge.
    hasRenderSnapshot: overrides.hasRenderSnapshot ?? m.renderSnapshotImage !== null,
    creator: m.creator,
    lastViewedAt: m.lastViewedAt,
    contentHash: m.contentHash,
    // The summary fixture is always a 3MF: no row or an old row means "unread".
    printDataState: m.printInfo?.readerCurrent ? m.printInfo.sliceState : 'unread',
    ...(m.printFacts ? { printFacts: m.printFacts } : {}),
    license: m.license,
    sourcePlatform: m.sourcePlatform,
    sourceModelId: m.sourceModelId,
    sourceProfileId: m.sourceProfileId,
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

export function makePrinter(overrides: Partial<Printer> = {}): Printer {
  return {
    id: 'printer-1', name: 'X1C', kind: 'filament', units: [],
    manufacturer: null, model: null, nozzleMm: null, bedXMm: 256, bedYMm: 256, bedZMm: 256,
    ...overrides,
  };
}
