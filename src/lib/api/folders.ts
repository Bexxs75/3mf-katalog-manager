import { invoke } from '@tauri-apps/api/core';
import type { Folder, FolderTrashResult, FolderTrashSummary } from '../../types';

export function listFolders() {
  return invoke<Folder[]>('list_folders');
}
export function createFolder(parentId: string | null, name: string) {
  return invoke('create_folder', { parentId, name });
}
export function moveFileToFolder(fileId: string, folderId: string) {
  return invoke('move_file_to_folder', { fileId, folderId });
}
// Per-file outcome of a batch move. `reason` is a stable code: skipped ->
// name_exists | already_there; failed -> invalid_id | not_found | in_trash |
// source_missing | sensitive_path | error. `fromFolderId`/`fromPath` are only
// set for "moved" (undo = another call with the old folder per file; a file that
// had no folder cannot be restored by it).
export interface MoveFileResult {
  id: string;
  status: 'moved' | 'skipped' | 'failed';
  reason?: string;
  fromFolderId?: string;
  fromPath?: string;
}
export function moveFilesToFolder(fileIds: string[], folderId: string) {
  return invoke<MoveFileResult[]>('move_files_to_folder', { fileIds, folderId });
}
export function moveFolder(folderId: string, newParentId: string) {
  return invoke('move_folder', { folderId, newParentId });
}
export function renameFolder(folderId: string, name: string) {
  return invoke('rename_folder', { folderId, name });
}
export function folderTrashSummary(folderId: string) {
  return invoke<FolderTrashSummary>('folder_trash_summary', { folderId });
}
export function trashFolder(folderId: string) {
  return invoke<FolderTrashResult>('trash_folder', { folderId });
}
