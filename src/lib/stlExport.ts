import { fileTypeOfName } from './slicerLinks';

export type StlEntryState = 'hidden' | 'disabled' | 'enabled';

/** Catalog file type of a model: its own type, the name's ending only as a fallback. */
export function modelFileType(model: { fileType?: string; name?: string; path?: string }): string {
  return (model.fileType ?? fileTypeOfName(model.path ?? model.name ?? '') ?? '').toLowerCase();
}

/**
 * Whether "Save as STL" is offered. Only the desktop app on the user's own computer can open a
 * save dialog; a STL has nothing to convert; STEP needs a build with the STEP preview (the same
 * check the viewer makes), so without it the entry is shown disabled with a hint.
 */
export function stlEntryState(options: { fileType: string; readOnly: boolean; container: boolean; hasStepPreview: boolean | null }): StlEntryState {
  const { fileType, readOnly, container, hasStepPreview } = options;
  if (readOnly || container) return 'hidden';
  if (fileType === '3mf' || fileType === 'obj') return 'enabled';
  if (fileType === 'stp' || fileType === 'step') return hasStepPreview === false ? 'disabled' : 'enabled';
  return 'hidden';
}
