import { listen } from '@tauri-apps/api/event';
import type { ImportJobResult, ImportProgress, ImportState, StartedImport } from '../../types';
import { invoke } from '@tauri-apps/api/core';
import type { ArchiveImportResult, ArchiveInfo, ArchiveRequest, ImportResultDto } from '../../types';

export function importFiles(targetFolderId?: string) {
  return targetFolderId === undefined ? invoke<ImportResultDto>('import_files') : invoke<ImportResultDto>('import_files', { targetFolderId });
}
export function importFolder() {
  return invoke<ImportResultDto>('import_folder');
}
export function importDropped(paths: string[]) {
  return invoke<ImportResultDto>('import_dropped', { paths });
}
export function exportCatalog(settingsJson: string) {
  return invoke<boolean>('export_catalog', { settingsJson });
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

export function startImport(source: 'files' | 'folder', targetFolderId?: string) {
  return invoke<StartedImport>('start_import', { source, targetFolderId });
}
export function startDroppedImport(paths: string[]) {
  return invoke<StartedImport>('start_dropped_import', { paths });
}
export function startAdoptImport(path: string) {
  return invoke<StartedImport>('start_adopt_import', { path });
}
export function startArchiveImport(targetDir: string, requests: ArchiveRequest[], deleteArchives: boolean, parentJobId?: string) {
  return invoke<StartedImport>('start_archive_import', { targetDir, requests, deleteArchives, parentJobId });
}
export function cancelImport(jobId: string) {
  return invoke<{ state: ImportState } | null>('cancel_import', { jobId });
}
export function getImportResult(jobId: string) {
  return invoke<ImportJobResult | null>('get_import_result', { jobId });
}
export function getImportState(jobId: string) {
  return invoke<ImportProgress | null>('get_import_state', { jobId });
}
export function onImportProgress(handler: (progress: ImportProgress) => void) {
  return listen<ImportProgress>('import://progress', event => handler(event.payload));
}
export function onImportFinished(handler: (result: ImportJobResult) => void) {
  return listen<ImportJobResult>('import://finished', event => handler(event.payload));
}

export function discardArchiveImports(paths: string[]) {
  return invoke<void>('discard_archive_imports', { paths });
}
