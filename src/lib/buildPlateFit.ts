import type { PrinterDetails } from '../types';

/** Width (X), depth (Y), height (Z) in millimetres. */
export type Dimensions3 = [number, number, number];

export type PlateFitAxis = 'width' | 'depth' | 'height';

export type PlateFit =
  | { status: 'unknown' }
  | { status: 'fits' }
  /** `excess` lists every axis that is too large, with the model's extent on it (rounded up). */
  | { status: 'tooLarge'; excess: { axis: PlateFitAxis; modelMm: number }[] };

const AXES: PlateFitAxis[] = ['width', 'depth', 'height'];

function isPositive(value: number | null | undefined): value is number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0;
}

/** Build volume of a printer, or null when any of the three sizes is missing or zero. */
export function printerBuildVolume(printer: Pick<PrinterDetails, 'bedXMm' | 'bedYMm' | 'bedZMm'>): Dimensions3 | null {
  const { bedXMm, bedYMm, bedZMm } = printer;
  if (!isPositive(bedXMm) || !isPositive(bedYMm) || !isPositive(bedZMm)) return null;
  return [bedXMm, bedYMm, bedZMm];
}

/** Model size, or null when unknown (missing, zero or not a number on any axis). */
export function usableModelSize(dimensionsMm: readonly number[] | null | undefined): Dimensions3 | null {
  if (!dimensionsMm || dimensionsMm.length !== 3) return null;
  const [x, y, z] = dimensionsMm;
  if (!isPositive(x) || !isPositive(y) || !isPositive(z)) return null;
  return [x, y, z];
}

// Tolerance for float noise from the geometry parser (a 256.0000001 mm model on a 256 mm bed).
const EPSILON = 1e-6;

/**
 * Does the model fit the build volume? The model may be turned around the
 * Z axis on the plate, so width and depth may swap; the height stays the
 * height (tipping it over would change the print and is not assumed).
 * When it does not fit, the orientation with the fewest (then smallest)
 * overshoots is reported. Unknown model size gives no verdict.
 */
export function checkPlateFit(dimensionsMm: readonly number[] | null | undefined, bed: Dimensions3): PlateFit {
  const size = usableModelSize(dimensionsMm);
  if (!size) return { status: 'unknown' };
  const [w, d, h] = size;
  const orientations: Dimensions3[] = [[w, d, h], [d, w, h]];
  let best: { excess: PlateFitAxis[]; overshoot: number; model: Dimensions3 } | null = null;
  for (const model of orientations) {
    const excess = AXES.filter((_, i) => model[i] > bed[i] + EPSILON);
    if (excess.length === 0) return { status: 'fits' };
    const overshoot = AXES.reduce((sum, _, i) => sum + Math.max(0, model[i] - bed[i]), 0);
    if (!best || excess.length < best.excess.length
      || (excess.length === best.excess.length && overshoot < best.overshoot)) {
      best = { excess, overshoot, model };
    }
  }
  const chosen = best!;
  return {
    status: 'tooLarge',
    // Rounded up so the shown number is visibly above the bed size it is compared with.
    excess: chosen.excess.map(axis => ({ axis, modelMm: Math.ceil(chosen.model[AXES.indexOf(axis)] - EPSILON) })),
  };
}

export interface PrinterPlateFit<P> {
  printer: P;
  bed: Dimensions3;
  fit: PlateFit;
}

/** One entry per printer that has a build volume; printers without one are left out. */
export function plateFitForPrinters<P extends PrinterDetails>(
  dimensionsMm: readonly number[] | null | undefined,
  printers: readonly P[],
): PrinterPlateFit<P>[] {
  return printers.flatMap(printer => {
    const bed = printerBuildVolume(printer);
    return bed ? [{ printer, bed, fit: checkPlateFit(dimensionsMm, bed) }] : [];
  });
}

/** Catalog filter predicate: models without known size stay visible. */
export function modelMayFitBed(dimensionsMm: readonly number[] | null | undefined, bed: Dimensions3): boolean {
  return checkPlateFit(dimensionsMm, bed).status !== 'tooLarge';
}
