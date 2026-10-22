import { isReadOnly } from '../readOnly';
import { fileOp, fileOpsBatch } from '../fileOps';
import { invoke } from '../transport';
import type { Folder, FolderTrashResult, FolderTrashSummary } from '../../types';

export function listFolders() {
  return invoke<Folder[]>('list_folders');
}
export function createFolder(parentId: string | null, name: string) {
  if (isReadOnly()) {
    if (!parentId) return Promise.reject(new Error('A parent folder is required.'));
    return fileOp('create_folder', { parentId, name });
  }
  return invoke('create_folder', { parentId, name });
}
export function moveFileToFolder(fileId: string, folderId: string) {
  if (isReadOnly()) return fileOpsBatch('move_files_to_folder', [fileId], { folderId });
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
// Web build: the server answers with plain per-file results; fileOpsBatch turns failures into errors.
export function moveFilesToFolderWeb(fileIds: string[], folderId: string) {
  return fileOpsBatch('move_files_to_folder', fileIds, { folderId });
}
export function moveFolder(folderId: string, newParentId: string) {
  if (isReadOnly()) return fileOp('move_folder', { folderId, newParentId });
  return invoke('move_folder', { folderId, newParentId });
}
export function renameFolder(folderId: string, name: string) {
  return isReadOnly() ? fileOp('rename_folder', { folderId, name }) : invoke('rename_folder', { folderId, name });
}
export function folderTrashSummary(folderId: string) {
  return invoke<FolderTrashSummary>('folder_trash_summary', { folderId });
}
export function trashFolder(folderId: string) {
  return invoke<FolderTrashResult>('trash_folder', { folderId });
}
