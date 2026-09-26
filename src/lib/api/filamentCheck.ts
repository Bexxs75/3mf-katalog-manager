import { invoke } from '@tauri-apps/api/core';
import type { FilamentCheck } from '../../types';

/** Checks the models in the given order (queue: shared deduction state). */
export function checkFilament(fileIds: string[]) {
  return invoke<FilamentCheck[]>('check_filament', { fileIds });
}
