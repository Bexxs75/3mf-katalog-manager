import { invoke } from '@tauri-apps/api/core';
import type { TagCount, CreatorCount, SavedFilter } from '../../types';
import type { SlicerDto } from './slicer';

export function listTagCounts() {
  return invoke<TagCount[]>('list_tag_counts');
}
export function listCreators() {
  return invoke<CreatorCount[]>('list_creators');
}
export function listSavedFilters() {
  return invoke<SavedFilter[]>('list_saved_filters');
}
// Loest serverseitig die bestehende Autoerkennung aus UND traegt neu
// gefundene Slicer direkt in die `registered_slicers`-Registry ein (M-06,
// Task 11) - gibt die vollstaendige, aktuelle Registry zurueck (nicht nur
// die neu gefundenen Eintraege).
export function scanInstalledSlicers() {
  return invoke<SlicerDto[]>('scan_installed_slicers');
}
