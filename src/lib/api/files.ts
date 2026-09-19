import { invoke } from '@tauri-apps/api/core';
import type { ModelFile, ModelFileSummary } from '../../types';

export function listFiles() {
  return invoke<ModelFile[]>('list_files');
}
// Schlanke Katalog-Uebersicht (Finding M-01) - siehe ModelFileSummary.
export function listFileSummaries() {
  return invoke<ModelFileSummary[]>('list_file_summaries');
}
// Nachtrag zu Finding M-01: list_file_summaries liefert bewusst keine Tags
// mehr (sonst wieder ein Pro-Zeile-Join) - diese eine Aggregat-Abfrage
// liefert alle Datei->Tag-Zuordnungen auf einen Schlag (file id -> tags[]),
// damit die Sidebar-Tag-Filterung fuer noch nicht einzeln geoeffnete
// Modelle weiterhin korrekt funktioniert (siehe useCatalogStore.ts).
export function listAllFileTags() {
  return invoke<Record<string, string[]>>('list_all_file_tags');
}
// Generischer Nachlade-Command fuer volle Modelldaten (Bilder/Materialien/
// Tags/Metadata) - die Detailseite ruft dies mit einer Liste der Laenge 1
// auf ([id]), kein separater Einzeldatensatz-Command (siehe Task-6-Brief).
export function listFilesByIds(ids: string[]) {
  return invoke<ModelFile[]>('list_files_by_ids', { ids });
}
export function listTrash() {
  return invoke<ModelFile[]>('list_trash');
}
export function markFileViewed(fileId: string) {
  return invoke('mark_file_viewed', { fileId });
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
export function setRenderSnapshot(fileId: string, imageBase64: string) {
  return invoke('set_render_snapshot', { fileId, imageBase64 });
}
export function setSourceUrl(fileId: string, url: string | null) {
  return invoke('set_source_url', { fileId, url });
}
export function rescanFileMetadata(fileId: string) {
  return invoke<ModelFile>('rescan_file_metadata', { fileId });
}
