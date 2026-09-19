import { invoke } from '@tauri-apps/api/core';
import type { Folder } from '../../types';

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
