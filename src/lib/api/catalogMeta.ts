import { invoke } from '@tauri-apps/api/core';
import type { TagCount } from '../../types';
import type { SlicerDto } from './slicer';

export function listTagCounts() {
  return invoke<TagCount[]>('list_tag_counts');
}
// Auto-detection in the backend; adds new slicers to the registry and returns
// the complete registry.
export function scanInstalledSlicers() {
  return invoke<SlicerDto[]>('scan_installed_slicers');
}
