import type { FilamentSpool } from '../types';

const idOrder = (a: FilamentSpool, b: FilamentSpool) => Number(a.id) - Number(b.id);

/**
 * Which of several equal spools to take first: the one with the smallest rest that still
 * covers `neededAmount` (a start amount of 0 only skips empty spools), so full spools stay
 * untouched. When none covers it, the fullest non-empty one is the closest fit. `null` when
 * all are empty. Ties go to the older spool (lower id). This only preselects; callers must
 * leave the choice to the user.
 */
export function suggestSpool(
  candidates: readonly FilamentSpool[],
  neededAmount = 0,
): FilamentSpool | null {
  const usable = candidates.filter((s) => s.remainingWeightG > 0);
  const covering = usable.filter((s) => s.remainingWeightG >= neededAmount);
  const pool = covering.length > 0 ? covering : usable;
  if (pool.length === 0) return null;
  const sorted = [...pool].sort((a, b) =>
    covering.length > 0
      ? a.remainingWeightG - b.remainingWeightG || idOrder(a, b)
      : b.remainingWeightG - a.remainingWeightG || idOrder(a, b),
  );
  return sorted[0];
}
