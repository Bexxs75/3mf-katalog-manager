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

export type FolderMoveBlock = 'catalogRoot' | 'self' | 'descendant' | 'alreadyThere' | 'nameTaken';

function pathBasename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? '';
}

/**
 * Why moving `folderId` below `targetId` cannot work, or null when it can.
 * Lets the UI explain a refused drop instead of silently doing nothing; the
 * backend repeats the binding checks against the real disk. Name clashes are
 * compared exactly: whether "a" and "A" clash depends on the file system, which
 * only the backend can see.
 */
export function folderMoveBlock(
  folderId: string,
  targetId: string,
  folders: Folder[],
  catalogBaseDir: string | null,
): FolderMoveBlock | null {
  const folder = folders.find((f) => f.id === folderId);
  const target = folders.find((f) => f.id === targetId);
  if (!folder || !target) return null;
  if (containsCatalogRoot(folder.path, catalogBaseDir)) return 'catalogRoot';
  if (folderId === targetId) return 'self';
  if (isFolderSelfOrDescendant(targetId, folderId, folders)) return 'descendant';
  if (folder.parentId === targetId) return 'alreadyThere';
  const name = pathBasename(folder.path);
  const clash = folders.some((f) => f.parentId === targetId && f.id !== folderId && pathBasename(f.path) === name);
  return clash ? 'nameTaken' : null;
}

/**
 * The folder every given file already lies in, or null when they differ (or there are
 * none). A drop target only counts as "already there" when ALL dragged files are in it:
 * the backend skips those and still moves the rest.
 */
export function sharedFolderId(models: ReadonlyArray<{ id: string; folderId?: string | null }>, ids: readonly string[]): string | null {
  let shared: string | null = null;
  for (const [index, id] of ids.entries()) {
    const folderId = models.find((model) => model.id === id)?.folderId ?? null;
    if (folderId === null || (index > 0 && folderId !== shared)) return null;
    shared = folderId;
  }
  return shared;
}
