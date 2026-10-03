import { invoke } from '@tauri-apps/api/core';
import type { Language } from '../../i18n/types';

/** Whether this build can render STEP geometry at all (see `has_step_preview`). */
export function hasStepPreview() {
  return invoke<boolean>('has_step_preview');
}

/** Opens the website's download section with the STEP variant preselected. */
export function openStepDownload(lang: Language) {
  return invoke('open_step_download', { lang });
}
