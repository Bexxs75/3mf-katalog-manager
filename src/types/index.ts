export type Origin = 'local';

export type SyncStatus = 'local-only';

export interface FilamentUsage {
  type: string;
  color: string | null;
  usedG: number;
  usedM: number;
}

export interface PlateFilamentUsage {
  plateIndex: number;
  weightG: number;
  filaments: FilamentUsage[];
}

export interface SliceInfo {
  totalWeightG: number;
  plates: PlateFilamentUsage[];
}

export interface CostEstimate {
  totalCost: number | null;
  hasUnpricedFilaments: boolean;
}

export interface ModelFile {
  id: string;
  name: string;
  path: string;
  folderId: string;
  tags: string[];
  origin: Origin;
  sync: SyncStatus;
  dimensionsMm: [number, number, number] | null;
  volumeCm3: number | null;
  objectCount: number | null;
  plateCount: number | null;
  materials: { name: string; displayColor: string | null }[];
  fileSizeBytes: number;
  importedAt: string;
  printStatus: 'not_printed' | 'printed';
  estimatedWeightG: number | null;
  weightSource: 'slicer' | 'estimated';
  sliceInfo: SliceInfo | null;
  costEstimate: CostEstimate | null;
  lastViewedAt: string | null;
  creator: string | null;
  customImage: string | null;
  thumbnailImage: string | null;
  renderSnapshotImage: string | null;
  sourceUrl: string | null;
  queuePosition: number | null;
  favorite: boolean;
  deletedAt: string | null;
}

export interface Folder {
  id: string;
  name: string;
  path: string;
  parentId: string | null;
  count: number;
}

export interface ImportResultDto {
  imported: ModelFile[];
  duplicateCount: number;
}

export interface TagCount {
  label: string;
  count: number;
  colorHue: number;
}

export interface CreatorCount {
  label: string;
  count: number;
}

export interface SavedFilter {
  id: string;
  name: string;
  folderId: string | null;
  tag: string | null;
  creator: string | null;
  query: string | null;
  sort: SortKey;
}

export type ViewMode = 'grid' | 'groupedGrid' | 'groupedList';
export type SortKey = 'name' | 'date' | 'size' | 'vol' | 'viewed';

export interface SlicerConfig {
  id: string;
  name: string;
  path: string;
  source: 'manual' | 'auto';
}

export interface FilamentSpool {
  id: string;
  material: string;
  manufacturer: string | null;
  color: string | null;
  location: string | null;
  diameterMm: number;
  originalWeightG: number;
  remainingWeightG: number;
  price: number | null;
  imagePng: string | null;
}

export interface CatalogIssues {
  orphaned: ModelFile[];
  duplicateGroups: ModelFile[][];
}

export interface Collection {
  id: string;
  name: string;
  modelCount: number;
}
