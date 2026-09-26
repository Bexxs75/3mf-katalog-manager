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
// Auto-detection in the backend; adds new slicers to the registry and returns
// the complete registry.
export function scanInstalledSlicers() {
  return invoke<SlicerDto[]>('scan_installed_slicers');
}
