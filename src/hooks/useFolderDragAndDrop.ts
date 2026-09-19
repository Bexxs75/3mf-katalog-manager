import { useCallback, useEffect, useState } from 'react';
import * as foldersApi from '../lib/api/folders';
import { isFolderSelfOrDescendant } from '../lib/folderTree';
import type { ModelFile, Folder } from '../types';

interface Refreshers {
  refreshFolders: () => void;
  refreshFiles: () => void;
}

export function useFolderDragAndDrop(models: ModelFile[], folders: Folder[], { refreshFolders, refreshFiles }: Refreshers) {
  const [draggedFileId, setDraggedFileId] = useState<string | null>(null);
  const [draggedFolderId, setDraggedFolderId] = useState<string | null>(null);
  const [dragOverFolderId, setDragOverFolderId] = useState<string | null>(null);
  const [moveToast, setMoveToast] = useState<{ from: string; to: string; error?: boolean } | null>(null);

  const onDragFileStart = useCallback((id: string) => setDraggedFileId(id), []);
  const onDragFolderStart = useCallback((id: string) => setDraggedFolderId(id), []);
  const dismissMoveToast = useCallback(() => setMoveToast(null), []);

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

  const onCreateFolder = useCallback(
    (parentId: string | null, name: string) =>
      foldersApi
        .createFolder(parentId, name)
        .then(() => refreshFolders())
        .catch((e) => {
          console.error('[folders] Anlegen fehlgeschlagen:', e);
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
    onCreateFolder,
  };
}
