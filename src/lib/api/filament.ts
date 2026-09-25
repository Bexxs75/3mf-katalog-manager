import { invoke } from '@tauri-apps/api/core';
import type { FilamentSpool } from '../../types';

/** Legt `count` neue, volle Eintraege nach dem Vorbild von `templateId` an (eine Transaktion). `weight`: g bzw. ml. */
export function restockFilamentSpool(
  templateId: string,
  count: number,
  weight: number,
  price: number | null,
  location: string | null,
) {
  return invoke<FilamentSpool[]>('restock_filament_spool', { templateId, count, weight, price, location });
}
