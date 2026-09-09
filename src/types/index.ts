export type Origin = 'local' | 'gdrive' | 'onedrive' | 'dropbox' | 'proton';

export type SyncStatus = 'synced' | 'outdated' | 'local-only' | 'cloud-only';

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
  materials: { name: string; displayColor: string | null }[];
  fileSizeBytes: number;
  importedAt: string;
}

export interface Folder {
  id: string;
  name: string;
  count: number;
}

export interface TagCount {
  label: string;
  count: number;
  colorHue: number;
}

export interface CloudAccount {
  id: Origin;
  abbr: string;
  name: string;
  status: 'connected' | 'error' | 'disconnected';
  usedPercent: number;
  quotaLabel: string;
}

export type ViewMode = 'grid' | 'list';
export type SortKey = 'name' | 'date' | 'size' | 'vol';

export interface SlicerConfig {
  id: string;
  name: string;
  path: string;
}
