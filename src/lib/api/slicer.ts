import { invoke } from '@tauri-apps/api/core';

export interface SlicerDto {
  id: string;
  name: string;
  executablePath: string;
}

// The file dialog runs in the backend; the frontend never passes a path through.
export function pickAndRegisterSlicer() {
  return invoke<SlicerDto | null>('pick_and_register_slicer');
}

export function listRegisteredSlicers() {
  return invoke<SlicerDto[]>('list_registered_slicers');
}

// Only the id of a registered slicer; path and validation live in the backend.
export function openInSlicer(modelId: string, slicerId: string) {
  return invoke('open_in_slicer', { fileId: modelId, slicerId });
}
