import { invoke } from '@tauri-apps/api/core';
import type { ArchiveImportResult, ArchiveInfo, ArchiveRequest, ImportResultDto } from '../../types';

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

export function inspectArchives(paths: string[]) {
  return invoke<ArchiveInfo[]>('inspect_archives', { paths });
}
export function archiveTargetConflicts(targetDir: string, folderNames: string[]) {
  return invoke<boolean[]>('archive_target_conflicts', { targetDir, folderNames });
}
export function extractArchives(targetDir: string, requests: ArchiveRequest[], deleteArchives: boolean) {
  return invoke<ArchiveImportResult>('extract_archives', { targetDir, requests, deleteArchives });
}
export function pickFolderPath() {
  return invoke<string | null>('pick_folder_path');
}
