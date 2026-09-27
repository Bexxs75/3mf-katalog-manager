import { invoke } from '@tauri-apps/api/core';

export function getAppVersion() {
  return invoke<string>('get_app_version');
}

export function openReleaseUrl(url: string) {
  return invoke('open_release_url', { url });
}
