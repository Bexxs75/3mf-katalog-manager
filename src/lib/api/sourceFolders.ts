import { invoke } from '../transport';
import type { SourceFolder, SourceScan, StartedImport } from '../../types';

export function listSourceFolders() {
  return invoke<SourceFolder[]>('list_source_folders');
}
// `path` must come from pickFolderPath (the folder the user chose in the dialog).
export function addSourceFolder(path: string) {
  return invoke<SourceFolder>('add_source_folder', { path });
}
// Forgets the folder; its files are never touched.
export function removeSourceFolder(id: string) {
  return invoke<void>('remove_source_folder', { id });
}
export function getSourceCheckOnStart() {
  return invoke<boolean>('get_source_check_on_start');
}
export function setSourceCheckOnStart(enabled: boolean) {
  return invoke<void>('set_source_check_on_start', { enabled });
}
// New model files per source folder; reads only.
export function scanSourceFolders() {
  return invoke<SourceScan[]>('scan_source_folders');
}
// Imports the chosen files through the normal import job (they are moved into the catalog folder).
export function importSourceFiles(paths: string[]) {
  return invoke<StartedImport>('import_source_files', { paths });
}
// Do not offer these files again (matched by path and size). Returns how many were remembered.
export function rejectSourceFiles(paths: string[]) {
  return invoke<number>('reject_source_files', { paths });
}
export function listRejectedCount() {
  return invoke<number>('list_rejected_count');
}
// Offers all declined files again. Returns how many were forgotten.
export function resetRejectedSourceFiles() {
  return invoke<number>('reset_rejected_source_files');
}
