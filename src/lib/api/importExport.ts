import { invoke } from '@tauri-apps/api/core';
import type { ImportResultDto } from '../../types';

export function importFiles() {
  return invoke<ImportResultDto>('import_files');
}
export function importFolder() {
  return invoke<ImportResultDto>('import_folder');
}
export function importDropped(paths: string[]) {
  return invoke<ImportResultDto>('import_dropped', { paths });
}
export function exportCatalog(settingsJson: string) {
  return invoke('export_catalog', { settingsJson });
}
export function importCatalog() {
  return invoke<{ imported: boolean; settingsJson: string | null }>('import_catalog');
}
