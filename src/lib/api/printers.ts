import { invoke } from '@tauri-apps/api/core';
import type { MaterialUnit, Printer, PrinterKind, UnitKind } from '../../types';

export function listPrinters() {
  return invoke<Printer[]>('list_printers');
}
/**
 * Legt den Drucker samt erster Einheit an: Filament-Drucker bekommen einen
 * Spulenhalter (1 Fach), Resin-Drucker ihre Harzwanne. `holderName` ist der
 * uebersetzte Name dieser Einheit.
 */
export function addPrinter(name: string, holderName: string, kind: PrinterKind) {
  return invoke<Printer>('add_printer', { name, holderName, kind });
}
export function renamePrinter(printerId: string, name: string) {
  return invoke('rename_printer', { printerId, name });
}
/** Liefert die Anzahl der Spulen, die an ihren Stammplatz zurueckkehrten. */
export function deletePrinter(printerId: string) {
  return invoke<number>('delete_printer', { printerId });
}
export function addUnit(printerId: string, kind: UnitKind, name: string, slotCount: number | null) {
  return invoke<MaterialUnit>('add_unit', { printerId, kind, name, slotCount });
}
export function updateUnit(unitId: string, name: string, slotCount: number | null) {
  return invoke<number>('update_unit', { unitId, name, slotCount });
}
export function deleteUnit(unitId: string) {
  return invoke<number>('delete_unit', { unitId });
}
export function reorderUnits(printerId: string, unitIds: string[]) {
  return invoke('reorder_units', { printerId, unitIds });
}
export function loadSpool(spoolId: string, unitId: string, slotIndex: number) {
  return invoke<{ displacedSpoolId: string | null }>('load_spool', { spoolId, unitId, slotIndex });
}
/** Liefert den neuen Lagerort (Stammplatz oder `location`). */
export function unloadSpool(spoolId: string, location: string | null) {
  return invoke<string | null>('unload_spool', { spoolId, location });
}
