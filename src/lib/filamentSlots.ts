import type { FilamentSpool, MaterialUnit } from '../types';

export function slotKey(unitId: string, slotIndex: number): string {
  return `${unitId}:${slotIndex}`;
}

/** Spools sitting in a slot, keyed by `slotKey`. */
export function spoolsBySlot(spools: FilamentSpool[]): Map<string, FilamentSpool> {
  const map = new Map<string, FilamentSpool>();
  for (const spool of spools) {
    if (spool.unitId !== null && spool.slotIndex !== null) {
      map.set(slotKey(spool.unitId, spool.slotIndex), spool);
    }
  }
  return map;
}

export function isInStorage(spool: FilamentSpool): boolean {
  return spool.unitId === null;
}

/** Short label for toasts, e.g. "PLA Black". */
export function spoolLabel(spool: FilamentSpool): string {
  return [spool.material, spool.color].filter(Boolean).join(' ');
}

/**
 * Does the entry fit this unit? Resin bottles only into a resin vat,
 * filament spools never (the backend checks the same in `load_spool`).
 */
export function spoolFitsUnit(spool: Pick<FilamentSpool, 'kind'>, unit: Pick<MaterialUnit, 'kind'>): boolean {
  return (spool.kind === 'resin') === (unit.kind === 'resin_vat');
}
