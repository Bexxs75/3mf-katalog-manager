import type { FilamentSpool } from '../types';

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
