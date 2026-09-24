import { invoke } from '@tauri-apps/api/core';
import type { FilamentCheck } from '../../types';

/** Prueft die Modelle in der uebergebenen Reihenfolge (Warteschlange: gemeinsamer Abbuchungsstand). */
export function checkFilament(fileIds: string[]) {
  return invoke<FilamentCheck[]>('check_filament', { fileIds });
}
