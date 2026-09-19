import { invoke } from '@tauri-apps/api/core';

export interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseUrl: string;
}

export function checkForUpdate() {
  return invoke<UpdateCheckResult>('check_for_update');
}

export function openReleaseUrl(url: string) {
  return invoke('open_release_url', { url });
}
