import type { FilamentSpool, SpoolKind } from '../types';

export const DEFAULT_SPOOL_WEIGHT_G = 1000;

export type WeightSuggestion =
  | { weight: number; source: 'manufacturerMaterial'; manufacturer: string; material: string }
  | { weight: number; source: 'manufacturer'; manufacturer: string }
  | { weight: number; source: 'default' };

const norm = (text: string | null | undefined) => (text ?? '').trim().toLowerCase();

/** Newest spool = highest id (ids grow with every insert). */
function newest(spools: FilamentSpool[]): FilamentSpool | undefined {
  return spools.reduce<FilamentSpool | undefined>(
    (best, s) => (best === undefined || Number(s.id) > Number(best.id) ? s : best),
    undefined,
  );
}

/**
 * Start weight for a new spool, taken from the stock itself: the newest spool with the
 * same manufacturer and material, else the newest of that manufacturer, else 1000 g.
 * Texts compare without case and surrounding spaces. Needs a manufacturer; spools of the
 * other kind (filament vs. resin) are never used.
 */
export function suggestSpoolWeight(
  spools: FilamentSpool[],
  kind: SpoolKind,
  manufacturer: string,
  material: string,
): WeightSuggestion {
  const maker = norm(manufacturer);
  if (maker !== '') {
    const ofMaker = spools.filter((s) => s.kind === kind && norm(s.manufacturer) === maker);
    const exact = material.trim() === '' ? undefined : newest(ofMaker.filter((s) => norm(s.material) === norm(material)));
    if (exact) {
      return { weight: exact.originalWeightG, source: 'manufacturerMaterial', manufacturer: exact.manufacturer ?? '', material: exact.material };
    }
    const byMaker = newest(ofMaker);
    if (byMaker) return { weight: byMaker.originalWeightG, source: 'manufacturer', manufacturer: byMaker.manufacturer ?? '' };
  }
  return { weight: DEFAULT_SPOOL_WEIGHT_G, source: 'default' };
}
