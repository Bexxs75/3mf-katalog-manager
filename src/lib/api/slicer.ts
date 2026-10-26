import { invoke } from '../transport';

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

export interface SlicerLinkDto {
  /** Relative URL of the signed download route. */
  path: string;
  /** Unix seconds after which the link stops working. */
  expiresAt: number;
  name: string;
}

// Web only: the browser cannot start a program, so the server signs a short-lived download link instead.
export function createSlicerLink(fileId: string) {
  return invoke<SlicerLinkDto>('slicer_link', { fileId });
}
