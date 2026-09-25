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
  contentHash: string | null;
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
  lastViewedAt: string | null;
  contentHash: string | null;
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
  pendingArchives?: string[];
}

export type ArchiveStatus = 'ok' | 'noModels' | 'tooLarge' | 'encrypted' | 'unreadable' | 'unsupported';

export interface ArchiveInfo {
  path: string;
  suggestedFolderName: string;
  modelCount: number;
  entryCount: number;
  unpackedSize: number;
  fileSize: number;
  modifiedUnixMs: number;
  status: ArchiveStatus;
}

export type ConflictMode = 'new' | 'merge';

export interface ArchiveRequest {
  path: string;
  folderName: string;
  onConflict: ConflictMode;
  expectedSize: number;
  expectedModifiedUnixMs: number;
}

export interface ArchiveOutcome {
  path: string;
  extractedTo: string | null;
  existingSkipped: number;
  unsafeSkipped: number;
  blockedSkipped: number;
  archiveDeleted: boolean;
  deleteError: string | null;
  error: string | null;
}

export interface ArchiveImportResult {
  imported: ModelFile[];
  duplicateCount: number;
  archives: ArchiveOutcome[];
}

export interface ArchiveProgress {
  path: string;
  state: 'extracting' | 'importing' | 'done' | 'failed';
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

/** Art eines Lager-Eintrags (v0.13.1). Bei 'resin' sind originalWeightG/remainingWeightG Milliliter. */
export type SpoolKind = 'filament' | 'resin';

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
  /** Farbwert `#rrggbb`; `color` bleibt der Farbname. */
  colorHex: string | null;
  /** Stammplatz, solange die Spule in einem Fach steckt (dann ist `location` leer). */
  homeLocation: string | null;
  unitId: string | null;
  slotIndex: number | null;
  kind: SpoolKind;
}

export type UnitKind =
  | 'bambu_ams'
  | 'bambu_ams_lite'
  | 'bambu_ams_ht'
  | 'creality_cfs'
  | 'prusa_mmu3'
  | 'anycubic_ace'
  | 'external'
  | 'custom'
  /** Harzwanne eines Resin-Druckers (v0.14.0): 1 Platz, fest, nur Resin. */
  | 'resin_vat';

/** Druckerart (v0.14.0), beim Anlegen gewaehlt und danach fest. */
export type PrinterKind = 'filament' | 'resin';

export interface MaterialUnit {
  id: string;
  printerId: string;
  name: string;
  kind: UnitKind;
  slotCount: number;
  bambuAmsIndex: number | null;
}

export interface Printer {
  id: string;
  name: string;
  kind: PrinterKind;
  units: MaterialUnit[];
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

export type FilamentCheckStatus = 'ok' | 'swap' | 'short' | 'unknown' | 'no_data';

export interface FilamentSlotRef {
  printer: string;
  unit: string;
  slotNumber: number;
}

export interface FilamentSpoolUse {
  spoolId: string;
  label: string;
  colorName: string | null;
  remainingG: number;
  originalG: number;
  slot: FilamentSlotRef | null;
  location: string | null;
}

export interface FilamentNeedCheck {
  filamentType: string;
  color: string | null;
  neededG: number;
  status: Exclude<FilamentCheckStatus, 'no_data'>;
  missingG: number;
  spools: FilamentSpoolUse[];
  possible: FilamentSpoolUse[];
}

export interface FilamentCheck {
  fileId: string;
  status: FilamentCheckStatus;
  needs: FilamentNeedCheck[];
}

/**
 * `'disabled'` kommt nur von `test_printer_connection` zurueck, wenn der
 * Schalter "Druckeranbindung" aus ist (Netzwerk-Sperre greift vor jeder
 * Anfrage, siehe global-constraints.md).
 */
export type PrinterConnectionError =
  | 'unreachable'
  | 'auth_required'
  | 'bad_response'
  | 'history_missing'
  | 'address_not_allowed'
  | 'disabled';

export interface PrinterConnection {
  printerId: string;
  kind: 'moonraker';
  address: string;
  baseUrl: string | null;
  remoteVersion: string | null;
  /** Unix-Sekunden: ab hier wird abgebucht. */
  connectedSince: number;
  lastSyncedAt: number | null;
  lastError: PrinterConnectionError | null;
  errorSince: number | null;
  paused: boolean;
}

export interface PrinterTestResult {
  ok: boolean;
  error: PrinterConnectionError | null;
  connection: PrinterConnection | null;
}

export interface PrinterJob {
  id: string;
  printerId: string;
  printerName: string;
  fileName: string;
  outcome: 'completed' | 'partial';
  rawStatus: string;
  endedAt: number;
  printDurationS: number;
  usedMm: number;
  partialPercent: number | null;
  material: string | null;
  hasThumbnail: boolean;
  suggestedSpoolId: string | null;
  grams: number | null;
  materialMismatch: boolean;
  modelMatch: { fileId: string; fileName: string; sure: boolean } | null;
}

export interface JobDecision {
  jobId: string;
  spoolId: string;
  fileId: string | null;
}

export interface JobPreview {
  grams: number;
  materialMismatch: boolean;
}

export interface ConfirmResult {
  confirmed: number;
  failed: number;
}
