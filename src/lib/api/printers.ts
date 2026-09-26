import { invoke } from '@tauri-apps/api/core';
import type { MaterialUnit, Printer, PrinterKind, UnitKind } from '../../types';

export function listPrinters() {
  return invoke<Printer[]>('list_printers');
}
/**
 * Creates the printer with its first unit: filament printers get a
 * spool holder (1 slot), resin printers their resin vat. `holderName` is the
 * translated name of that unit.
 */
export function addPrinter(name: string, holderName: string, kind: PrinterKind) {
  return invoke<Printer>('add_printer', { name, holderName, kind });
}
export function renamePrinter(printerId: string, name: string) {
  return invoke('rename_printer', { printerId, name });
}
/** Returns the number of spools that returned to their home location. */
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
/** Returns the new location (home location or `location`). */
export function unloadSpool(spoolId: string, location: string | null) {
  return invoke<string | null>('unload_spool', { spoolId, location });
}
