import { isReadOnly } from '../readOnly';
import { fileOp, fileOpsBatch } from '../fileOps';
import { invoke } from '../transport';
import type { Folder } from '../../types';

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
export function moveFolder(folderId: string, newParentId: string) {
  if (isReadOnly()) return fileOp('move_folder', { folderId, newParentId });
  return invoke('move_folder', { folderId, newParentId });
}

export function renameFolder(folderId: string, name: string) {
  return isReadOnly() ? fileOp('rename_folder', { folderId, name }) : invoke('rename_folder', { folderId, name });
}
