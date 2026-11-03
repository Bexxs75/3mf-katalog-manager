import type { ModelFile, PrintFacts } from '../types';
import { printDataStateOf, printFactsOf } from './printData';

/**
 * Filter by print data (target printer, nozzle, layer height, print time, color).
 *
 * A value that is not known is never guessed. A file without
 * the value of an active filter is neither matching nor not matching: it is "missing" and is
 * hidden (and counted) until the user chooses to show it. A value that differs is a plain "fail".
 * Inside one filter the choices are OR; between filters AND; colors OR/AND by `colorMode`.
 */

export type FilterKey = 'printer' | 'nozzle' | 'layer' | 'time' | 'color';
export const FILTER_KEYS: readonly FilterKey[] = ['printer', 'nozzle', 'layer', 'time', 'color'];

export type TimeBucket = 'lt1' | '1to4' | '4to12' | 'ge12';
export const TIME_BUCKETS: readonly TimeBucket[] = ['lt1', '1to4', '4to12', 'ge12'];

export type ColorId = 'black' | 'white' | 'gray' | 'red' | 'orange' | 'yellow' | 'green' | 'blue' | 'brown' | 'violet';
export const COLOR_IDS: readonly ColorId[] = ['black', 'white', 'gray', 'red', 'orange', 'yellow', 'green', 'blue', 'brown', 'violet'];

/** Representative swatch of each color group (the mockup's eight plus brown and violet/pink). */
export const COLOR_SWATCH: Readonly<Record<ColorId, string>> = {
  black: '#1c1c1c', white: '#f2f2f0', gray: '#8a8d91', red: '#c8322d', orange: '#e8832a',
  yellow: '#e9c93a', green: '#3f9a5a', blue: '#2f6fc4', brown: '#7a4a2b', violet: '#8a4fb5',
};

export interface PrintDataFilter {
  /** Lower-cased, trimmed target printer names (see `printerKey`). */
  printers: string[];
  /** Nozzle diameters rounded to 2 decimals. */
  nozzles: number[];
  /** Layer heights rounded to 3 decimals. */
  layers: number[];
  times: TimeBucket[];
  colors: ColorId[];
  colorMode: 'or' | 'and';
  /** Also show the files that lack a value of an active filter (marked on the card). */
  includeUnknown: boolean;
}

export const EMPTY_FILTER: PrintDataFilter = {
  printers: [], nozzles: [], layers: [], times: [], colors: [], colorMode: 'or', includeUnknown: false,
};

/** True when at least one of the five filters has a choice. `includeUnknown` alone does not count. */
export function isActive(filter: PrintDataFilter): boolean {
  return filter.printers.length > 0 || filter.nozzles.length > 0 || filter.layers.length > 0
    || filter.times.length > 0 || filter.colors.length > 0;
}

/** Which of the five filters have a choice. */
export function activeKeys(filter: PrintDataFilter): FilterKey[] {
  const lists: Record<FilterKey, number> = {
    printer: filter.printers.length, nozzle: filter.nozzles.length, layer: filter.layers.length,
    time: filter.times.length, color: filter.colors.length,
  };
  return FILTER_KEYS.filter(key => lists[key] > 0);
}

// --- keys ---------------------------------------------------------------------------------------

/** Same printer in a different spelling of case or spaces is one choice. Slicer names are not unified. */
export function printerKey(name: string | undefined): string | null {
  const key = name?.trim().toLowerCase() ?? '';
  return key === '' ? null : key;
}

/** Float noise (`0.4000000059604645`) must not make a second nozzle. */
export function nozzleKey(mm: number): number {
  return Math.round(mm * 100) / 100;
}

/** `0.2` and `0.20000000298` are the same layer height. */
export function layerKey(mm: number): number {
  return Math.round(mm * 1000) / 1000;
}

/** Lower limit included, upper limit not. */
export function timeBucketOf(seconds: number): TimeBucket {
  if (seconds < 3600) return 'lt1';
  if (seconds < 14400) return '1to4';
  if (seconds < 43200) return '4to12';
  return 'ge12';
}

/**
 * Color group of `#rrggbb` (an `#rrggbbaa` is read without the alpha). Null for anything else.
 * HSL based: little saturation is black, gray or white by lightness; otherwise the hue picks the
 * group (red 335-15, orange 15-42, yellow 42-70, green 70-165, blue 165-255, violet/pink 255-335).
 * Dark reddish and orange hues are brown, a light red is pink (violet).
 */
export function colorBucket(hex: string): ColorId | null {
  const match = /^#([0-9a-f]{6})(?:[0-9a-f]{2})?$/i.exec(hex.trim());
  if (!match) return null;
  const value = parseInt(match[1], 16);
  const r = ((value >> 16) & 255) / 255;
  const g = ((value >> 8) & 255) / 255;
  const b = (value & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const lightness = (max + min) / 2;
  const chroma = max - min;
  const saturation = chroma === 0 ? 0 : chroma / (1 - Math.abs(2 * lightness - 1));
  if (lightness < 0.12) return 'black';
  if (saturation < 0.15) {
    if (lightness < 0.2) return 'black';
    return lightness > 0.88 ? 'white' : 'gray';
  }
  let hue: number;
  if (max === r) hue = ((g - b) / chroma) % 6;
  else if (max === g) hue = (b - r) / chroma + 2;
  else hue = (r - g) / chroma + 4;
  hue = (hue * 60 + 360) % 360;
  if (lightness > 0.93) return 'white';
  if (hue >= 15 && hue < 42) return lightness < 0.45 ? 'brown' : 'orange';
  if (hue >= 335 || hue < 15) {
    if (lightness < 0.35 && saturation < 0.6) return 'brown';
    return lightness >= 0.75 ? 'violet' : 'red';
  }
  if (hue < 70) return 'yellow';
  if (hue < 165) return 'green';
  if (hue < 255) return 'blue';
  return 'violet';
}

/** The color groups of a file's colors, in order of first occurrence. */
export function colorBucketsOf(colors: readonly string[] | undefined): ColorId[] {
  const seen = new Set<ColorId>();
  for (const color of colors ?? []) {
    const bucket = colorBucket(color);
    if (bucket) seen.add(bucket);
  }
  return [...seen];
}

// --- evaluation ---------------------------------------------------------------------------------

export interface Evaluation {
  /** A value is known and differs from the choice. */
  fail: boolean;
  /** Active filters whose value the file does not have. */
  missing: FilterKey[];
}

const PASS: Evaluation = { fail: false, missing: [] };

/** The nozzle of a file as a key; mixed or unknown is null (the single values are not stored). */
function nozzleOf(facts: PrintFacts | null): number | null {
  return facts && !facts.nozzleMixed && facts.nozzleMm !== undefined ? nozzleKey(facts.nozzleMm) : null;
}

/** Only a complete sum is a print time; a partial one would put a file in a wrong bucket. */
function timeOf(facts: PrintFacts | null): TimeBucket | null {
  return facts && facts.printSecondsComplete && facts.printSeconds !== undefined ? timeBucketOf(facts.printSeconds) : null;
}

export function evaluate(model: Pick<ModelFile, 'printInfo' | 'printFacts'>, filter: PrintDataFilter): Evaluation {
  if (!isActive(filter)) return PASS;
  const facts = printFactsOf(model);
  let fail = false;
  const missing: FilterKey[] = [];
  const single = <T>(key: FilterKey, chosen: readonly T[], value: T | null) => {
    if (chosen.length === 0) return;
    if (value === null) missing.push(key);
    else if (!chosen.includes(value)) fail = true;
  };
  single('printer', filter.printers, printerKey(facts?.printerModel));
  single('nozzle', filter.nozzles, nozzleOf(facts));
  single('layer', filter.layers, facts?.layerHeightMm !== undefined ? layerKey(facts.layerHeightMm) : null);
  single('time', filter.times, timeOf(facts));
  if (filter.colors.length > 0) {
    const have = colorBucketsOf(facts?.colors);
    if (have.length === 0) missing.push('color');
    else {
      const ok = filter.colorMode === 'or'
        ? filter.colors.some(color => have.includes(color))
        : filter.colors.every(color => have.includes(color));
      if (!ok) fail = true;
    }
  }
  return { fail, missing };
}

export interface PrintDataSplit<T> {
  /** Matching files, plus (with `includeUnknown`) those with a missing value. */
  visible: T[];
  /** Files with a missing value that are held back (not `includeUnknown`). */
  hidden: number;
  /** Files with a missing value that are shown anyway. */
  shownUnknown: number;
  /** Of the hidden files, those whose print data have not been read yet. */
  unread: number;
}

/** Applies the filter. Without an active filter the list is returned as it is. */
export function splitByPrintData<T extends Pick<ModelFile, 'printInfo' | 'printFacts' | 'path' | 'printDataState'>>(
  models: readonly T[],
  filter: PrintDataFilter,
): PrintDataSplit<T> {
  if (!isActive(filter)) return { visible: models as T[], hidden: 0, shownUnknown: 0, unread: 0 };
  const visible: T[] = [];
  let hidden = 0;
  let shownUnknown = 0;
  let unread = 0;
  for (const model of models) {
    const { fail, missing } = evaluate(model, filter);
    if (fail) continue;
    if (missing.length === 0) visible.push(model);
    else if (filter.includeUnknown) {
      visible.push(model);
      shownUnknown++;
    } else {
      hidden++;
      if (printDataStateOf(model) === 'unread') unread++;
    }
  }
  return { visible, hidden, shownUnknown, unread };
}

// --- choices of the popovers ----------------------------------------------------------------------

export interface Facet<K> { key: K; count: number }
export interface PrinterFacet extends Facet<string> { label: string }
export interface Facets {
  printers: PrinterFacet[];
  nozzles: Facet<number>[];
  layers: Facet<number>[];
  times: Facet<TimeBucket>[];
  colors: Facet<ColorId>[];
  /** Files without the value, per filter. */
  missing: Record<FilterKey, number>;
}

/**
 * What the popovers offer: only values that occur, with the number of files that have them in the
 * whole catalog (a file with several colors counts once per color group). One pass over the list.
 */
export function printDataFacets(models: readonly Pick<ModelFile, 'printInfo' | 'printFacts'>[]): Facets {
  const printers = new Map<string, { count: number; spellings: Map<string, number> }>();
  const nozzles = new Map<number, number>();
  const layers = new Map<number, number>();
  const times = new Map<TimeBucket, number>();
  const colors = new Map<ColorId, number>();
  const missing: Record<FilterKey, number> = { printer: 0, nozzle: 0, layer: 0, time: 0, color: 0 };
  for (const model of models) {
    const facts = printFactsOf(model);
    const key = printerKey(facts?.printerModel);
    if (key === null) missing.printer++;
    else {
      const entry = printers.get(key) ?? { count: 0, spellings: new Map() };
      entry.count++;
      const spelling = facts!.printerModel!.trim();
      entry.spellings.set(spelling, (entry.spellings.get(spelling) ?? 0) + 1);
      printers.set(key, entry);
    }
    const nozzle = nozzleOf(facts);
    if (nozzle === null) missing.nozzle++; else nozzles.set(nozzle, (nozzles.get(nozzle) ?? 0) + 1);
    const layer = facts?.layerHeightMm !== undefined ? layerKey(facts.layerHeightMm) : null;
    if (layer === null) missing.layer++; else layers.set(layer, (layers.get(layer) ?? 0) + 1);
    const time = timeOf(facts);
    if (time === null) missing.time++; else times.set(time, (times.get(time) ?? 0) + 1);
    const have = colorBucketsOf(facts?.colors);
    if (have.length === 0) missing.color++;
    else for (const color of have) colors.set(color, (colors.get(color) ?? 0) + 1);
  }
  return {
    printers: [...printers].map(([key, entry]) => ({
      key,
      count: entry.count,
      // The most frequent spelling is shown; a tie goes to the one sorting first, so the label is stable.
      label: [...entry.spellings].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0][0],
    })).sort((a, b) => a.label.localeCompare(b.label)),
    nozzles: [...nozzles].map(([key, count]) => ({ key, count })).sort((a, b) => a.key - b.key),
    layers: [...layers].map(([key, count]) => ({ key, count })).sort((a, b) => a.key - b.key),
    times: TIME_BUCKETS.filter(key => times.has(key)).map(key => ({ key, count: times.get(key) ?? 0 })),
    colors: COLOR_IDS.filter(key => colors.has(key)).map(key => ({ key, count: colors.get(key) ?? 0 })),
    missing,
  };
}
