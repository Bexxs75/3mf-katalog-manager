import type { ModelFile } from '../types';

const WIKI_BASE = 'https://wiki.3mfkatalog.de';

/** The wiki exists in German and English; Spanish and French readers get the English page. */
export function filamentUsageHelpUrl(language: string): string {
  return language === 'de' ? `${WIKI_BASE}/help/filament-usage/` : `${WIKI_BASE}/en/help/filament-usage/`;
}

/** Only 3MF can carry slicer data; STL, OBJ and STEP never have a filament usage. */
export function canHaveFilamentUsage(path: string): boolean {
  return path.toLowerCase().endsWith('.3mf');
}

/**
 * True when a 3MF has no slicer usage data, so the detail page should say
 * why. Never while the list summary is still standing in for the full
 * record, because its `sliceInfo` is empty only for lack of loading.
 */
export function isFilamentUsageMissing(model: Pick<ModelFile, 'path' | 'sliceInfo' | 'summaryOnly'>): boolean {
  return canHaveFilamentUsage(model.path) && !model.summaryOnly && model.sliceInfo === null;
}
