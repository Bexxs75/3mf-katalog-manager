import { invoke } from '@tauri-apps/api/core';
import type { FilamentSpool } from '../../types';

/** Creates `count` new, full entries modeled on `templateId` (one transaction). `weight`: g or ml. */
export function restockFilamentSpool(
  templateId: string,
  count: number,
  weight: number,
  price: number | null,
  location: string | null,
) {
  return invoke<FilamentSpool[]>('restock_filament_spool', { templateId, count, weight, price, location });
}

/** Deducts used milliliters from a resin bottle (never below 0) and returns the new amount. */
export function consumeResin(spoolId: string, amountMl: number) {
  return invoke<FilamentSpool>('consume_resin', { spoolId, amountMl });
}

/** Reads an image dropped via drag & drop (only drops observed by the backend, max. 5 MB) as Base64. */
export function readDroppedImage(path: string) {
  return invoke<string>('read_dropped_image', { path });
}
