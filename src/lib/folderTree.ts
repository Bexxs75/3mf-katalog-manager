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

/** Moving an ancestor invalidates the persisted import destination as well. */
export function containsCatalogRoot(path: string, catalogBaseDir: string | null): boolean {
  if (!catalogBaseDir || !path) return false;
  const windows = /^[a-z]:[/\\]/i.test(catalogBaseDir) || catalogBaseDir.startsWith('\\\\');
  const normalize = (value: string) => {
    const separated = (windows ? value.replace(/\\/g, '/') : value).replace(/\/+$/, '');
    return windows ? separated.toLowerCase() : separated;
  };
  const root = normalize(catalogBaseDir);
  const folder = normalize(path);
  return root === folder || root.startsWith(`${folder}/`);
}
