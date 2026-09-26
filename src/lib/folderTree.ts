import type { Folder } from '../types';

/**
 * Checks whether a file lies in `targetId` itself or one of its descendants
 * (recursively along the `parentId` chain in `folders`). Analogous to the
 * backend `count` semantics: a click on a parent folder also shows the
 * files from all subfolders.
 */
export function isFileInFolderOrDescendant(
  fileFolderId: string | null,
  targetId: string,
  folders: Folder[],
): boolean {
  if (fileFolderId === null) return false;
  let current: string | null = fileFolderId;
  while (current !== null) {
    if (current === targetId) return true;
    current = folders.find((f) => f.id === current)?.parentId ?? null;
  }
  return false;
}

/**
 * Whether `candidateId` equals `ancestorId` or is a descendant of it. Only for
 * the highlight while dragging; `is_descendant` in the backend does the binding check.
 */
export function isFolderSelfOrDescendant(
  candidateId: string,
  ancestorId: string,
  folders: Folder[],
): boolean {
  let current: string | null = candidateId;
  while (current !== null) {
    if (current === ancestorId) return true;
    current = folders.find((f) => f.id === current)?.parentId ?? null;
  }
  return false;
}
