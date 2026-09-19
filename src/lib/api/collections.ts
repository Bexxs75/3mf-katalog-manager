import { invoke } from '@tauri-apps/api/core';
import type { Collection, ModelFile } from '../../types';

export function listCollections() {
  return invoke<Collection[]>('list_collections');
}
export function listCollectionFiles(collectionId: string) {
  return invoke<ModelFile[]>('list_collection_files', { collectionId });
}
export function createCollection(name: string) {
  return invoke<Collection>('create_collection', { name });
}
export function renameCollection(collectionId: string, name: string) {
  return invoke('rename_collection', { collectionId, name });
}
export function deleteCollection(collectionId: string) {
  return invoke('delete_collection', { collectionId });
}
export function addFilesToCollection(collectionId: string, fileIds: string[]) {
  return invoke('add_files_to_collection', { collectionId, fileIds });
}
export function removeFileFromCollection(collectionId: string, fileId: string) {
  return invoke('remove_file_from_collection', { collectionId, fileId });
}
export function reorderCollection(collectionId: string, updates: { fileId: string; position: number }[]) {
  return invoke('reorder_collection', { collectionId, updates });
}
