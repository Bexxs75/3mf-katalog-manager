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

/**
 * Schlanke Projektion von `ModelFile` fuer die Katalog-Uebersicht (Grid/
 * Liste), gespeist von `list_file_summaries` (Finding M-01: die volle
 * `ModelFile`-Abfrage laedt pro Zeile Materials/Tags/Metadata sowie das
 * grosse `customImage`-Zusatzbild, obwohl die Uebersicht nur eine Kachel-
 * Vorschau braucht). Enthaelt bewusst KEIN `materials`/`tags`/`customImage`/
 * `sliceInfo`/`costEstimate`/`sourceUrl`/`lastViewedAt` - `thumbnailImage`,
 * `renderSnapshotImage` UND `creator` bleiben dagegen enthalten, da Grid-
 * Vorschau bzw. Sidebar-/Suchfilter direkt auf diesen Feldern operieren
 * (Bugfix 2026-09-20: sowohl `renderSnapshotImage` als auch `creator` waren
 * hier zunaechst ausgeschlossen, siehe Kommentar an `db::FileSummary` im
 * Backend - ohne sie zeigte das Grid nie einen im Hintergrund bereits
 * gerenderten Snapshot bzw. lief der Creator-Filter fuer jedes nur per
 * Summary geladene Modell ins Leere, solange es nicht einzeln per
 * `listFilesByIds` nachgeladen wurde).
 * Volle Daten werden erst beim Oeffnen der Detailseite/Auswahl ueber
 * `listFilesByIds([id])` nachgeladen (siehe `useCatalogStore.ts`).
 */
export interface ModelFileSummary {
  id: string;
  name: string;
  path: string;
  folderId: string;
  fileType: string;
  fileSizeBytes: number;
  dimensionsMm: [number, number, number] | null;
  volumeCm3: number | null;
  objectCount: number | null;
  importedAt: string;
  printStatus: 'not_printed' | 'printed';
  favorite: boolean;
  queuePosition: number | null;
  thumbnailImage: string | null;
  renderSnapshotImage: string | null;
  // Praktisch redundant, seit renderSnapshotImage selbst mitgeliefert wird -
  // siehe Kommentar an `db::FileSummary::has_render_snapshot`.
  hasRenderSnapshot: boolean;
  creator: string | null;
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

// M-06 (Task 11): die Slicer-Registry lebt jetzt vollstaendig im Backend
// (`registered_slicers`-Tabelle) statt in localStorage - `id` ist seitdem
// eine echte, vom Backend vergebene Datenbank-id (als String), kein mehr
// client-seitig erzeugtes `crypto.randomUUID()`. `source` (manuell/
// automatisch erkannt) wird vom Backend nicht mehr an das Frontend
// zurueckgegeben - jeder registrierte Eintrag ist gleichermassen
// vertrauenswuerdig, sobald er in der Tabelle steht.
export interface SlicerConfig {
  id: string;
  name: string;
  path: string;
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
