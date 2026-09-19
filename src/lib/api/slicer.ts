import { invoke } from '@tauri-apps/api/core';

export function openInSlicer(slicerPath: string, filePath: string) {
  return invoke('open_in_slicer', { slicerPath, filePath });
}
