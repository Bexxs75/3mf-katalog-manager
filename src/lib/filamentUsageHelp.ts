import type { ModelFile } from '../types';

// Placeholder: the wiki start page until the manual page about exporting a
// sliced plate has its final address. Change it here only.
export const FILAMENT_USAGE_HELP_URL = 'https://wiki.3mfkatalog.de/';

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
