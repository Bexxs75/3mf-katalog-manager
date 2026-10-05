import { invoke } from '@tauri-apps/api/core';

/** Whether this build can render STEP geometry at all (see `has_step_preview`). */
export function hasStepPreview() {
  return invoke<boolean>('has_step_preview');
}
