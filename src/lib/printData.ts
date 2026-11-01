import type { FilamentUsage, ModelFile } from '../types';

/**
 * Display state of the print-data card. The first four are the stored slice
 * states; `partial` is derived (sliced, but a shown value is missing) and
 * `unread` covers a 3MF without a row or with a row of another reader version.
 */
export type PrintDataView = 'sliced' | 'no_slice_data' | 'not_recognized' | 'unreadable' | 'partial' | 'unread';

/** One shown value: known, "several different ones", or not found in the file (never a 0). */
export type SharedValue<T> = { kind: 'value'; value: T } | { kind: 'mixed' } | { kind: 'missing' };

export interface PlateRow {
  plateIndex: number;
  name: string | null;
  /** Estimated print time; the first-layer time is never used here. */
  printSeconds: number | null;
  weightG: number | null;
  filaments: FilamentUsage[];
}

export interface TimeSum {
  /** Sum of the seconds of the known plates (not of rounded minutes); null when none is known. */
  seconds: number | null;
  known: number;
  total: number;
}

export interface SharedRows {
  printerModel: SharedValue<string>;
  bedType: SharedValue<string>;
  layerHeightMm: SharedValue<number>;
  nozzleMm: SharedValue<number>;
  bedTempC: SharedValue<number>;
  nozzleTempC: SharedValue<number>;
}

export interface PrintDataSummary {
  state: PrintDataView;
  plates: PlateRow[];
  sum: TimeSum;
  shared: SharedRows;
  /**
   * `full`: the values of a sliced file; `project`: only printer, bed type, layer height and nozzle of a
   * file without print data whose settings were read (the source gives no temperatures there); `none`.
   */
  sharedKind: 'full' | 'project' | 'none';
  /** The project settings could not be read: a missing shared value says so instead of "not found". */
  settingsUnreadable: boolean;
}

const missing = { kind: 'missing' } as const;

const NO_SHARED: SharedRows = {
  printerModel: missing, bedType: missing, layerHeightMm: missing,
  nozzleMm: missing, bedTempC: missing, nozzleTempC: missing,
};

function text(value: string | null): SharedValue<string> {
  return value === null || value === '' ? missing : { kind: 'value', value };
}

function mixedNumber(value: { value: number | null; mixed: boolean }): SharedValue<number> {
  if (value.mixed) return { kind: 'mixed' };
  return value.value === null ? missing : { kind: 'value', value: value.value };
}

/** Time sum over plate rows: known plates only, in seconds. */
export function sumPrintTime(rows: readonly PlateRow[]): TimeSum {
  let seconds = 0;
  let known = 0;
  for (const row of rows) {
    if (row.printSeconds === null) continue;
    known += 1;
    seconds += row.printSeconds;
  }
  return { seconds: known === 0 ? null : seconds, known, total: rows.length };
}

/** Plate rows: the plates of the slicer usage, enriched by the name and time of the print data. */
function plateRows(model: Pick<ModelFile, 'sliceInfo' | 'printInfo'>, useInfo: boolean): PlateRow[] {
  const infoByIndex = new Map((useInfo ? model.printInfo?.plates ?? [] : []).map(p => [p.plateIndex, p]));
  if (model.sliceInfo) {
    return model.sliceInfo.plates.map(plate => {
      const info = infoByIndex.get(plate.plateIndex);
      return {
        plateIndex: plate.plateIndex,
        name: info?.name ?? null,
        printSeconds: info?.printSeconds ?? null,
        weightG: plate.weightG,
        filaments: plate.filaments,
      };
    });
  }
  return [...infoByIndex.values()].map(info => ({
    plateIndex: info.plateIndex, name: info.name, printSeconds: info.printSeconds, weightG: null, filaments: [],
  }));
}

/**
 * Reduces a model to what the print-data card shows. A row of another reader
 * version counts as not read: its values are never shown. Missing values stay
 * `missing` (never 0), `firstLayerSeconds` is never added or shown as a time.
 */
export function summarizePrintData(model: Pick<ModelFile, 'sliceInfo' | 'printInfo'>): PrintDataSummary {
  const info = model.printInfo;
  if (info === null || !info.readerCurrent) {
    const plates = plateRows(model, false);
    return { state: 'unread', plates, sum: sumPrintTime(plates), shared: NO_SHARED, sharedKind: 'none', settingsUnreadable: false };
  }
  const sliced = info.sliceState === 'sliced';
  const plates = plateRows(model, sliced);
  // `total` counts the plates that have slicer data (the usage); without a usage the plates of the row.
  const sum = sumPrintTime(plates);
  const project = info.sliceState === 'no_slice_data' && info.settingsState === 'read';
  const shared: SharedRows = sliced || project ? {
    printerModel: text(info.printerModel),
    bedType: text(info.bedType),
    layerHeightMm: info.layerHeightMm === null ? missing : { kind: 'value', value: info.layerHeightMm },
    nozzleMm: mixedNumber(info.nozzleMm),
    bedTempC: sliced ? mixedNumber(info.bedTempC) : missing,
    nozzleTempC: sliced ? mixedNumber(info.nozzleTempC) : missing,
  } : NO_SHARED;
  const sharedMissing = Object.values(shared).some(value => value.kind === 'missing');
  const state: PrintDataView = sliced && (sum.known < sum.total || sharedMissing) ? 'partial' : info.sliceState;
  return { state, plates, sum, shared, sharedKind: sliced ? 'full' : project ? 'project' : 'none', settingsUnreadable: sliced && info.settingsState === 'unreadable' };
}

/** Filaments of several plates merged by type and color. */
export function mergeFilaments(rows: readonly PlateRow[]): FilamentUsage[] {
  const merged = new Map<string, FilamentUsage>();
  for (const row of rows) {
    for (const filament of row.filaments) {
      const key = `${filament.type}|${filament.color ?? ''}`;
      const entry = merged.get(key);
      if (entry) {
        entry.usedG += filament.usedG;
        entry.usedM += filament.usedM;
      } else {
        merged.set(key, { ...filament });
      }
    }
  }
  return [...merged.values()];
}

/** Total weight of the rows; null when none has a weight. */
export function sumWeightG(rows: readonly PlateRow[]): number | null {
  const known = rows.filter(row => row.weightG !== null);
  return known.length === 0 ? null : known.reduce((total, row) => total + (row.weightG ?? 0), 0);
}

/** True for a 3MF whose print data are loaded in full (not a list summary, not a trash record without them). */
export function showsPrintData(model: Pick<ModelFile, 'path' | 'summaryOnly' | 'deletedAt'>): boolean {
  return model.path.toLowerCase().endsWith('.3mf') && !model.summaryOnly && model.deletedAt === null;
}
