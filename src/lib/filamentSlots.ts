import type { FilamentSpool, MaterialUnit } from '../types';

export function slotKey(unitId: string, slotIndex: number): string {
  return `${unitId}:${slotIndex}`;
}

/** Spulen, die in einem Fach stecken, nach `slotKey`. */
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

/** Kurzbezeichnung fuer Hinweise, z.B. "PLA Schwarz". */
export function spoolLabel(spool: FilamentSpool): string {
  return [spool.material, spool.color].filter(Boolean).join(' ');
}

/**
 * Passt der Eintrag in diese Einheit? Resin-Flaschen nur in eine Harzwanne,
 * Filament-Spulen nie (v0.14.0; das Backend prueft dasselbe in `load_spool`).
 */
export function spoolFitsUnit(spool: Pick<FilamentSpool, 'kind'>, unit: Pick<MaterialUnit, 'kind'>): boolean {
  return (spool.kind === 'resin') === (unit.kind === 'resin_vat');
}
