import { Channel, invoke } from '@tauri-apps/api/core';

export interface LastUpdate {
  version: string;
  date: string;
  backupFile: string;
}

export interface UpdateInfo {
  currentVersion: string;
  availableVersion: string | null;
  releaseUrl: string | null;
  canInstall: boolean;
  lastUpdate: LastUpdate | null;
}

export interface DownloadProgress {
  downloaded: number;
  total: number | null;
}

export const checkAppUpdate = () => invoke<UpdateInfo>('check_app_update');

export function downloadAppUpdate(onProgress: (p: DownloadProgress) => void) {
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;
  return invoke<string>('download_app_update', { onProgress: channel });
}

export const discardAppUpdate = () => invoke<void>('discard_app_update');

export const installAppUpdate = () => invoke<void>('install_app_update');
