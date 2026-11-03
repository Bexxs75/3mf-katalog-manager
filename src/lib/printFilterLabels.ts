import type { Translations } from '../i18n/types';
import type { Language } from '../i18n/types';
import { formatLayerHeightMm, formatNozzleMm } from '../i18n/format';
import {
  FILTER_KEYS, type ColorId, type Facets, type FilterKey, type PrintDataFilter, type TimeBucket,
} from './printDataFilter';

type TextKey = keyof Translations;

export const FILTER_LABEL_KEY: Readonly<Record<FilterKey, TextKey>> = {
  printer: 'pfFilterPrinter', nozzle: 'pfFilterNozzle', layer: 'pfFilterLayer', time: 'pfFilterTime', color: 'pfFilterColor',
};

export const FILTER_HINT_KEY: Readonly<Record<FilterKey, TextKey>> = {
  printer: 'pfPrinterHint', nozzle: 'pfNozzleHint', layer: 'pfLayerHint', time: 'pfTimeHint', color: 'pfColorHint',
};

export const TIME_LABEL_KEY: Readonly<Record<TimeBucket, TextKey>> = {
  lt1: 'pfTimeLt1', '1to4': 'pfTime1to4', '4to12': 'pfTime4to12', ge12: 'pfTimeGe12',
};

export const COLOR_LABEL_KEY: Readonly<Record<ColorId, TextKey>> = {
  black: 'pfColorBlack', white: 'pfColorWhite', gray: 'pfColorGray', red: 'pfColorRed', orange: 'pfColorOrange',
  yellow: 'pfColorYellow', green: 'pfColorGreen', blue: 'pfColorBlue', brown: 'pfColorBrown', violet: 'pfColorViolet',
};

/** Number of choices made in one filter (the badge on its button). */
export function choiceCount(filter: PrintDataFilter, key: FilterKey): number {
  return { printer: filter.printers, nozzle: filter.nozzles, layer: filter.layers, time: filter.times, color: filter.colors }[key].length;
}

/** The filter with the choices of one key removed (the colors keep their AND/OR mode). */
export function withoutChoices(filter: PrintDataFilter, key: FilterKey): PrintDataFilter {
  switch (key) {
    case 'printer': return { ...filter, printers: [] };
    case 'nozzle': return { ...filter, nozzles: [] };
    case 'layer': return { ...filter, layers: [] };
    case 'time': return { ...filter, times: [] };
    case 'color': return { ...filter, colors: [] };
  }
}

export interface PrintFilterChip {
  kind: 'pfFilterPrinter' | 'pfFilterNozzle' | 'pfFilterLayer' | 'pfFilterTime' | 'pfFilterColor' | 'pfFilterUnknown';
  value: string;
  /** Where the focus goes after the chip is removed. */
  focus: FilterKey | 'heading';
  remove: () => void;
}

/** One chip per active print data filter, and one for "files without print data are shown too". */
export function printFilterChips(
  filter: PrintDataFilter,
  facets: Facets | null,
  onChange: (next: PrintDataFilter) => void,
  t: <K extends TextKey>(key: K) => Translations[K],
  language: Language,
): PrintFilterChip[] {
  const text = (key: TextKey) => t(key) as string;
  const chips: PrintFilterChip[] = [];
  const printerLabels = new Map((facets?.printers ?? []).map(entry => [entry.key, entry.label]));
  const values: Record<FilterKey, string[]> = {
    printer: filter.printers.map(key => printerLabels.get(key) ?? key),
    nozzle: filter.nozzles.map(mm => formatNozzleMm(mm, language)),
    layer: filter.layers.map(mm => formatLayerHeightMm(mm, language)),
    time: filter.times.map(bucket => text(TIME_LABEL_KEY[bucket])),
    color: filter.colors.map(color => text(COLOR_LABEL_KEY[color])),
  };
  for (const key of FILTER_KEYS) {
    if (values[key].length === 0) continue;
    const joined = values[key].join(', ');
    chips.push({
      kind: FILTER_LABEL_KEY[key] as PrintFilterChip['kind'],
      value: key === 'color' ? `${joined} (${text(filter.colorMode === 'or' ? 'pfModeOr' : 'pfModeAnd')})` : joined,
      focus: key,
      remove: () => onChange(withoutChoices(filter, key)),
    });
  }
  if (chips.length > 0 && filter.includeUnknown) {
    chips.push({ kind: 'pfFilterUnknown', value: text('pfUnknownShown'), focus: 'heading', remove: () => onChange({ ...filter, includeUnknown: false }) });
  }
  return chips;
}
