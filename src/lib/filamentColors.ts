import type { UnitKind } from '../types';

/** Gleiche Werte wie `COLOR_NAMES` in src-tauri/src/db/printers.rs. */
export const FILAMENT_PALETTE: readonly string[] = [
  '#1a1a1a', '#f2f2f2', '#8a8d91', '#c0c0c0',
  '#c0392b', '#e67e22', '#f1c40f', '#27ae60',
  '#16a085', '#2e86de', '#8e44ad', '#e84393',
  '#8b5a2b', '#d4af37', '#e8d5b5', '#e8eef0',
];

export function isValidColorHex(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

export interface UnitTemplate {
  kind: UnitKind;
  /** Feste Fachanzahl; `null` = frei waehlbar ("Eigene…"). */
  slotCount: number | null;
  /** Vorschlag fuer den Namen der neuen Einheit. */
  defaultName: string;
}

/** Gleiche Fachanzahlen wie `template_slot_count` im Backend. */
export const UNIT_TEMPLATES: readonly UnitTemplate[] = [
  { kind: 'bambu_ams', slotCount: 4, defaultName: 'AMS' },
  { kind: 'bambu_ams_lite', slotCount: 4, defaultName: 'AMS lite' },
  { kind: 'bambu_ams_ht', slotCount: 1, defaultName: 'AMS HT' },
  { kind: 'creality_cfs', slotCount: 4, defaultName: 'CFS' },
  { kind: 'prusa_mmu3', slotCount: 5, defaultName: 'MMU3' },
  { kind: 'anycubic_ace', slotCount: 4, defaultName: 'ACE Pro' },
  // Name kommt uebersetzt aus i18n (printersKindExternal), siehe PrinterManagePanel.
  { kind: 'external', slotCount: 1, defaultName: '' },
  { kind: 'custom', slotCount: null, defaultName: '' },
];
