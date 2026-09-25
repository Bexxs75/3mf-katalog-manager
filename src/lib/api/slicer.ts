import { invoke } from '@tauri-apps/api/core';

export interface SlicerDto {
  id: string;
  name: string;
  executablePath: string;
}

// Der Datei-Dialog laeuft im Backend; das Frontend reicht nie einen Pfad durch.
export function pickAndRegisterSlicer() {
  return invoke<SlicerDto | null>('pick_and_register_slicer');
}

export function listRegisteredSlicers() {
  return invoke<SlicerDto[]>('list_registered_slicers');
}

// Nur die id eines registrierten Slicers; Pfad und Pruefung liegen im Backend.
export function openInSlicer(modelId: string, slicerId: string) {
  return invoke('open_in_slicer', { fileId: modelId, slicerId });
}
