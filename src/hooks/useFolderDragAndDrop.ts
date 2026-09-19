import { useCallback, useEffect, useState } from 'react';
import * as foldersApi from '../lib/api/folders';
import { isFolderSelfOrDescendant } from '../lib/folderTree';
import type { ModelFile, Folder } from '../types';

interface Refreshers {
  refreshFolders: () => void;
  refreshFiles: () => void;
}

export function useFolderDragAndDrop(models: ModelFile[], folders: Folder[], { refreshFolders, refreshFiles }: Refreshers) {
  // Maus-basiertes Drag-Tracking fuer physisches Verschieben von Dateien/
  // Ordnern (Task 7) - folgt demselben Muster wie der Warteschlangen-Reorder
  // in Sidebar.tsx und der Karten-Reorder in ModelGrid.tsx: kein natives
  // HTML5-DnD (draggable/onDragStart/onDragOver/onDrop), da Tauri/WebKitGTK
  // das nicht zuverlaessig unterstuetzt (dragDropEnabled faengt native
  // Drag-Sessions auf Fensterebene ab, siehe Kommentare dort).
  const [draggedFileId, setDraggedFileId] = useState<string | null>(null);
  const [draggedFolderId, setDraggedFolderId] = useState<string | null>(null);
  const [dragOverFolderId, setDragOverFolderId] = useState<string | null>(null);
  const [moveToast, setMoveToast] = useState<{ from: string; to: string; error?: boolean } | null>(null);

  const onDragFileStart = useCallback((id: string) => setDraggedFileId(id), []);
  // Baum-Zeile in FolderTree wird per Mousedown als Drag-Quelle markiert
  // (Ordner-auf-Ordner-Verschieben, Step 7). Ein einfacher Klick ohne
  // anschliessendes Hovern ueber eine andere Zeile loest nie einen Move aus,
  // da dragOverFolderId dann null bleibt (siehe Mouseup-Handler unten).
  const onDragFolderStart = useCallback((id: string) => setDraggedFolderId(id), []);
  const dismissMoveToast = useCallback(() => setMoveToast(null), []);

  // Baum-Zeile wird waehrend eines aktiven Drags (Datei oder Ordner)
  // betreten -> Drop-Ziel-Highlight setzen. Beim Ordner-Drag wird die
  // Zyklus-Vorabpruefung (eigener Unterbaum/sich selbst) hier clientseitig
  // dupliziert, damit gar kein Highlight auf einem ungueltigen Ziel
  // erscheint - die serverseitige Pruefung in move_folder (Task 5) bleibt
  // die verbindliche Instanz.
  const handleFolderMouseEnter = useCallback(
    (id: string) => {
      if (!draggedFileId && !draggedFolderId) return;
      if (draggedFolderId && isFolderSelfOrDescendant(id, draggedFolderId, folders)) {
        setDragOverFolderId(null);
        return;
      }
      setDragOverFolderId(id);
    },
    [draggedFileId, draggedFolderId, folders],
  );

  // Kopfzeile wird verlassen -> Highlight zuruecksetzen, sofern nicht
  // bereits eine andere Zeile inzwischen als Ziel gesetzt wurde (spaetes
  // mouseleave darf ein neueres mouseenter nicht ueberschreiben - siehe
  // Review-Fund I-1: ohne dieses Reset bleibt dragOverFolderId auf dem
  // zuletzt ueberfahrenen Ordner haengen, wenn danach ueber einer Karte
  // oder der "Ohne Ordner"-Sektion losgelassen wird, und loest dort einen
  // ungewollten move_file_to_folder in den falschen Ordner aus).
  const handleFolderMouseLeave = useCallback((id: string) => {
    setDragOverFolderId((current) => (current === id ? null : current));
  }, []);

  const onCreateFolder = useCallback(
    (parentId: string | null, name: string) =>
      foldersApi
        .createFolder(parentId, name)
        .then(() => refreshFolders())
        .catch((e) => {
          console.error('[folders] Anlegen fehlgeschlagen:', e);
          // Fehler sichtbar in der Naehe des Ordnerbaums zeigen (MoveToast
          // wiederverwendet mit error:true) statt nur in das Settings-only
          // catalogBackupError zu routen, das ohne geoeffnetes Rail-Panel
          // unsichtbar bleibt.
          setMoveToast({ from: name, to: String(e), error: true });
        }),
    [refreshFolders],
  );

  useEffect(() => {
    if (!draggedFileId) return;
    const handleMouseUp = () => {
      const fileId = draggedFileId;
      const folderId = dragOverFolderId;
      setDraggedFileId(null);
      setDragOverFolderId(null);
      if (!folderId) return;
      const file = models.find((m) => m.id === fileId);
      const targetFolder = folders.find((f) => f.id === folderId);
      if (!file || !targetFolder) return;
      foldersApi
        .moveFileToFolder(fileId, folderId)
        .then(() => {
          setMoveToast({ from: file.name, to: targetFolder.path });
          refreshFolders();
          refreshFiles();
        })
        .catch((e) => {
          console.error('[folders] Datei verschieben fehlgeschlagen:', e);
          setMoveToast({ from: file.name, to: String(e), error: true });
        });
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [draggedFileId, dragOverFolderId, models, folders, refreshFolders, refreshFiles]);

  // Analoger mouseup-Handler fuer das Verschieben eines Ordners per
  // Maus-Drag auf eine andere Baum-Zeile (Step 7). Die Zyklus-Pruefung wird
  // hier zusaetzlich wiederholt (nicht nur beim Hover-Highlight), damit ein
  // ungueltiges Ziel unter keinen Umstaenden einen invoke-Aufruf ausloest -
  // move_folder auf der Rust-Seite lehnt es ohnehin verbindlich ab.
  useEffect(() => {
    if (!draggedFolderId) return;
    const handleMouseUp = () => {
      const folderId = draggedFolderId;
      const targetId = dragOverFolderId;
      setDraggedFolderId(null);
      setDragOverFolderId(null);
      if (!targetId || targetId === folderId) return;
      if (isFolderSelfOrDescendant(targetId, folderId, folders)) return;
      const folder = folders.find((f) => f.id === folderId);
      const targetFolder = folders.find((f) => f.id === targetId);
      if (!folder || !targetFolder) return;
      foldersApi
        .moveFolder(folderId, targetId)
        .then(() => {
          setMoveToast({ from: folder.name, to: targetFolder.path });
          refreshFolders();
          refreshFiles();
        })
        .catch((e) => {
          console.error('[folders] Ordner verschieben fehlgeschlagen:', e);
          setMoveToast({ from: folder.name, to: String(e), error: true });
        });
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [draggedFolderId, dragOverFolderId, folders, refreshFolders, refreshFiles]);

  return {
    draggedFileId,
    draggedFolderId,
    dragOverFolderId,
    moveToast,
    dismissMoveToast,
    onDragFileStart,
    onDragFolderStart,
    handleFolderMouseEnter,
    handleFolderMouseLeave,
    onCreateFolder,
  };
}
