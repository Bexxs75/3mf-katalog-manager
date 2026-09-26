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
 * Slim projection of `ModelFile` for grid and list
 * (`list_file_summaries`), without the fields only the detail page needs
 * (e.g. `materials`, `customImage`, `sliceInfo`). `listFilesByIds([id])`
 * loads the full data on selection.
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
  // Redundant to renderSnapshotImage, see `db::FileSummary`.
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

export type ViewMode = 'grid' | 'groupedGrid' | 'groupedList';
export type SortKey = 'name' | 'date' | 'size' | 'vol' | 'viewed';

// Entry of the slicer registry in the backend; `id` is the database id.
export interface SlicerConfig {
  id: string;
  name: string;
  path: string;
}

/** Kind of an inventory entry. For 'resin', originalWeightG/remainingWeightG are milliliters. */
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
  /** Color value `#rrggbb`; `color` stays the color name. */
  colorHex: string | null;
  /** Home location while the spool sits in a slot (then `location` is empty). */
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
  /** Resin vat of a resin printer: 1 place, fixed, resin only. */
  | 'resin_vat';

/** Printer kind, chosen on creation and fixed afterwards. */
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
 * `'disabled'` only comes from `test_printer_connection` when the
 * "Printer connection" switch is off (then no request goes out).
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
  /** Unix seconds: deductions start from here. */
  connectedSince: number;
  lastSyncedAt: number | null;
  lastError: PrinterConnectionError | null;
  errorSince: number | null;
  paused: boolean;
  /** Printer clock minus local clock in seconds; 0 when within 5 minutes. */
  clockOffsetS: number;
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
