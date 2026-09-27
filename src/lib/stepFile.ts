// Mirrors the backend's own extension match in `get_model_geometry`
// ("stp" | "step"), since the frontend only ever sees a file path, never a
// file-type field for the full model record.
export function isStepFilePath(path: string): boolean {
  const dot = path.lastIndexOf('.');
  if (dot === -1) return false;
  const extension = path.slice(dot + 1).toLowerCase();
  return extension === 'stp' || extension === 'step';
}
