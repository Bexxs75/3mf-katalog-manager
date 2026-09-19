import { invoke } from '@tauri-apps/api/core';
import type { TagCount, CreatorCount, SavedFilter } from '../../types';

export function listTagCounts() {
  return invoke<TagCount[]>('list_tag_counts');
}
export function listCreators() {
  return invoke<CreatorCount[]>('list_creators');
}
export function listSavedFilters() {
  return invoke<SavedFilter[]>('list_saved_filters');
}
export function scanInstalledSlicers() {
  return invoke<{ name: string; path: string }[]>('scan_installed_slicers');
}
