import type { Folder } from '../types';
import { isCatalogRootFolder } from './folderTree';
import { toCssPosition, type Point } from './imageDrop';

/** Why a folder row under the pointer cannot take an Explorer drop. */
export type DropTargetBlock = 'missing' | 'catalogRoot';

/**
 * What lies under the pointer of an Explorer drag: no folder (the drop is catalogued in
 * place, as before), a folder that takes the files, or a folder that does not.
 */
export type DropTarget =
  | { kind: 'none' }
  | { kind: 'folder'; folder: Folder }
  | { kind: 'invalid'; folder: Folder; reason: DropTargetBlock };

export const noDropTarget: DropTarget = { kind: 'none' };

/** The catalog folder itself and folders gone from disk are no targets; the backend repeats the checks. */
export function dropTargetFor(folder: Folder, catalogBaseDir: string | null): DropTarget {
  if (folder.missing) return { kind: 'invalid', folder, reason: 'missing' };
  if (isCatalogRootFolder(folder.path, catalogBaseDir)) return { kind: 'invalid', folder, reason: 'catalogRoot' };
  return { kind: 'folder', folder };
}

/**
 * Looks up the folder row at a Tauri drop position (`data-folder-id` on the row). With a modal
 * dialog open no folder is a target, so a drop behind a dialog keeps the old behaviour.
 */
export function resolveExternalDropTarget(
  position: Point,
  devicePixelRatio: number,
  isWindows: boolean,
  folders: readonly Folder[],
  catalogBaseDir: string | null,
  doc: Document = document,
): DropTarget {
  if (doc.querySelector('[aria-modal="true"]')) return noDropTarget;
  const css = toCssPosition(position, devicePixelRatio, isWindows);
  const row = doc.elementFromPoint(css.x, css.y)?.closest('[data-folder-id]');
  const id = row?.getAttribute('data-folder-id');
  const folder = id ? folders.find((f) => f.id === id) : undefined;
  return folder ? dropTargetFor(folder, catalogBaseDir) : noDropTarget;
}
