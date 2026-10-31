import { invoke } from '../transport';
import type { ModelFile, ModelFileSummary, ModelImages } from '../../types';

// Slim catalog overview, see ModelFileSummary.
export function listFileSummaries() {
  return invoke<ModelFileSummary[]>('list_file_summaries');
}
export function listFileImages(ids: string[]) {
  return invoke<ModelImages[]>('list_file_images', { ids });
}
// All file-tag mappings in one query (the summaries contain no tags).
export function listAllFileTags() {
  return invoke<Record<string, string[]>>('list_all_file_tags');
}
// Load full model data; the detail page calls this with [id].
export function listFilesByIds(ids: string[]) {
  return invoke<ModelFile[]>('list_files_by_ids', { ids });
}
export function listTrash() {
  return invoke<ModelFile[]>('list_trash');
}
export function markFileViewed(fileId: string) {
  return invoke('mark_file_viewed', { fileId });
}
export function renameFile(fileId: string, name: string) {
  return invoke<void>('rename_file', { fileId, name });
}
export type DateRenameSource = 'modified' | 'imported';
export type DateRenameStatus =
  | 'rename' | 'skip_has_prefix' | 'skip_conflict' | 'skip_missing' | 'skip_trash'
  | 'skip_too_long' | 'skip_external' | 'skip_no_date' | 'skip_invalid';
export interface DateRenamePreview { id: string; oldName: string; newName: string; status: DateRenameStatus }
export interface DateRenameResult { id: string; status: 'renamed' | 'failed'; reason?: string }
// Mirrors the backend limit per call.
export const DATE_RENAME_MAX_ITEMS = 5000;
export function previewDateRename(fileIds: string[], dateSource: DateRenameSource) {
  return invoke<DateRenamePreview[]>('preview_date_rename', { fileIds, dateSource });
}
export function applyDateRename(items: { id: string; newName: string }[]) {
  return invoke<DateRenameResult[]>('apply_date_rename', { items });
}
export function deleteFile(fileId: string) {
  return invoke('delete_file', { fileId });
}
export function deleteFiles(fileIds: string[]) {
  return invoke('delete_files', { fileIds });
}
export function restoreFile(fileId: string) {
  return invoke('restore_file', { fileId });
}
export function deleteFilePermanently(fileId: string) {
  return invoke('delete_file_permanently', { fileId });
}
export function emptyTrash() {
  return invoke('empty_trash');
}
export function addTag(fileId: string, tag: string) {
  return invoke('add_tag', { fileId, tag });
}
export function removeTag(fileId: string, tag: string) {
  return invoke('remove_tag', { fileId, tag });
}
export function setPrintStatus(fileId: string, status: 'printed' | 'not_printed') {
  return invoke('set_print_status', { fileId, status });
}
export function setFavorite(fileId: string, favorite: boolean) {
  return invoke('set_favorite', { fileId, favorite });
}
export function addToQueue(fileId: string) {
  return invoke<number>('add_to_queue', { fileId });
}
export function removeFromQueue(fileId: string) {
  return invoke('remove_from_queue', { fileId });
}
export function reorderQueue(updates: { fileId: string; position: number }[]) {
  return invoke('reorder_queue', { updates });
}
export function uploadCustomImage(fileId: string) {
  return invoke<string | null>('upload_custom_image', { fileId });
}
export function setCustomImage(fileId: string, imageBase64: string) {
  return invoke('set_custom_image', { fileId, imageBase64 });
}
export function setRenderSnapshot(fileId: string, imageBase64: string) {
  return invoke('set_render_snapshot', { fileId, imageBase64 });
}
export function setSourceUrl(fileId: string, url: string | null) {
  return invoke('set_source_url', { fileId, url });
}
export function rescanFileMetadata(fileId: string) {
  return invoke<ModelFile>('rescan_file_metadata', { fileId });
}

export function setLicense(fileIds: string[], license: string | null): Promise<void> {
  return invoke('set_license', { fileIds, license });
}
