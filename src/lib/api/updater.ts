import { Channel } from '@tauri-apps/api/core';
import { invoke } from '../transport';

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
  /** The check itself failed (offline, firewall, ...). */
  checkFailed: boolean;
}

export interface DownloadProgress {
  downloaded: number;
  total: number | null;
}

export const checkAppUpdate = () => invoke<UpdateInfo>('check_app_update');

export function downloadAppUpdate(onProgress: (p: DownloadProgress) => void) {
  if (import.meta.env.VITE_TARGET === 'web') return Promise.reject({ message: 'Updater unavailable in web', expected: true });
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;
  return invoke<string>('download_app_update', { onProgress: channel });
}

export const discardAppUpdate = () => invoke<void>('discard_app_update');

export const installAppUpdate = () => invoke<void>('install_app_update');

export type UpdateChannel = 'stable' | 'rc';
export const getUpdateChannel = () => invoke<UpdateChannel>('get_update_channel');
export const setUpdateChannel = (channel: UpdateChannel) => invoke<void>('set_update_channel', { channel });
