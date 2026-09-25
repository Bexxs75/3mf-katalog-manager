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

/** Zieht verbrauchte Milliliter von einer Resin-Flasche ab (nie unter 0) und liefert den neuen Stand. */
export function consumeResin(spoolId: string, amountMl: number) {
  return invoke<FilamentSpool>('consume_resin', { spoolId, amountMl });
}

/** Liest ein per Drag & Drop abgelegtes Bild (nur vom Backend beobachtete Drops, max. 5 MB) als Base64. */
export function readDroppedImage(path: string) {
  return invoke<string>('read_dropped_image', { path });
}
