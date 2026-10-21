import { invoke } from '@tauri-apps/api/core';
import type { Companion, CompanionOpenInfo } from '../../types';

// Limits mirrored from the backend, for hints in the UI. The backend enforces them.
export const MAX_COMPANION_BYTES = 50 * 1024 * 1024;
export const MAX_COMPANIONS_PER_MODEL = 100;

// All companions of a model, trashed ones included (they carry `deletedAt`).
export function listCompanions(fileId: string) {
  return invoke<Companion[]>('list_companions', { fileId });
}
// File dialog for companions; the picked paths are what add_companion accepts next.
export function pickCompanionFiles() {
  return invoke<string[]>('pick_companion_files');
}
// Copies a file the user picked (or dropped on the window) into the model's companion folder.
export function addCompanion(fileId: string, sourcePath: string) {
  return invoke<Companion>('add_companion', { fileId, sourcePath });
}
// Moves the companion into the app trash.
export function removeCompanion(companionId: string) {
  return invoke<void>('remove_companion', { companionId });
}
export function restoreCompanion(companionId: string) {
  return invoke<Companion>('restore_companion', { companionId });
}
// Path and kind, only for companions that still pass the checks for an allowed type.
export function companionOpenInfo(companionId: string) {
  return invoke<CompanionOpenInfo>('companion_open_info', { companionId });
}
// Opens the companion with the system's default program (by id, never by path).
export function openCompanion(companionId: string) {
  return invoke<void>('open_companion', { companionId });
}
// The picture as a data URI (images only, size-capped).
export function companionImageData(companionId: string) {
  return invoke<string>('companion_image_data', { companionId });
}
