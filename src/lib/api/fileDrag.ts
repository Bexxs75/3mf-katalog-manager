import { invoke } from '../transport';

// Prototype P5-7: dragging catalog files out of the window. Off by default; the backend refuses
// the drag while the switch is off and takes file ids, never paths.
export function getDragOutEnabled() {
  return invoke<boolean>('get_drag_out_enabled');
}
export function setDragOutEnabled(enabled: boolean) {
  return invoke<void>('set_drag_out_enabled', { enabled });
}
export function startFileDrag(fileIds: string[]) {
  return invoke<void>('start_file_drag', { fileIds });
}
