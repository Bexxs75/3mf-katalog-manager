import type { Folder } from '../types';

/**
 * Prueft, ob eine Datei in `targetId` selbst oder einem seiner Nachfahren
 * liegt (rekursiv entlang der `parentId`-Kette in `folders`). Analog zur
 * Backend-`count`-Semantik: ein Klick auf einen Elternordner zeigt auch die
 * Dateien aus allen Unterordnern.
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
 * Ob `candidateId` gleich `ancestorId` oder ein Nachfahre davon ist. Nur fuer
 * das Highlight beim Ziehen; verbindlich prueft `is_descendant` im Backend.
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
