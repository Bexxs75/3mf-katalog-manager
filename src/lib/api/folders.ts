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
