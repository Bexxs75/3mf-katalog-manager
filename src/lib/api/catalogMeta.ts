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
// Autoerkennung im Backend; traegt neue Slicer in die Registry ein und gibt
// die vollstaendige Registry zurueck.
export function scanInstalledSlicers() {
  return invoke<SlicerDto[]>('scan_installed_slicers');
}
