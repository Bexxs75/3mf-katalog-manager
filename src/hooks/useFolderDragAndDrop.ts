import { dismissDragDropTip } from '../lib/dragDropTip';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import * as collectionsApi from '../lib/api/collections';
import * as foldersApi from '../lib/api/folders';
import { folderMoveBlock, sharedFolderId } from '../lib/folderTree';
import { messageOf, toAppError } from '../lib/errors';
import {
  groupByOldFolder, pushUndoStep, summarizeMoveResults, undoItemsFromResults,
  type MoveToastState, type UndoOutcome, type UndoStep,
} from '../lib/moveUndo';
import type { ModelFile, Folder, Collection } from '../types';

interface Refreshers {
  refreshFolders: () => void;
  refreshFiles: () => void;
}

interface CollectionTargets {
  collections: Collection[];
  addModelToCollection: (fileId: string, collectionId: string) => Promise<unknown>;
}

export function useFolderDragAndDrop(models: ModelFile[], folders: Folder[], { refreshFolders, refreshFiles }: Refreshers, collectionTargets?: CollectionTargets, catalogBaseDir: string | null = null, selectedIds?: ReadonlySet<string>) {
  // Mouse-based dragging instead of HTML5 DnD: under Tauri/WebKitGTK
  // dragDropEnabled intercepts native drag sessions at window level.
  const [draggedFileId, setDraggedFileId] = useState<string | null>(null);
  const [draggedFolderId, setDraggedFolderId] = useState<string | null>(null);
  const [dragOverFolderId, setDragOverFolderId] = useState<string | null>(null);
  const [dragOverCollectionId, setDragOverCollectionId] = useState<string | null>(null);
  // Everything that travels with the dragged card: the whole selection when the
  // card is part of it, otherwise just the card.
  const [draggedFileIds, setDraggedFileIds] = useState<string[]>([]);
  const [moveToast, setMoveToast] = useState<MoveToastState | null>(null);
  // In memory only (gone after a restart); the ref is the source of truth so a quick
  // second action never reads a stale list, the counter only drives re-renders.
  const undoStack = useRef<UndoStep[]>([]);
  const [undoCount, setUndoCount] = useState(0);
  const undoBusy = useRef(false);
  const draggedFilesFolderId = useMemo(() => sharedFolderId(models, draggedFileIds), [models, draggedFileIds]);
  // Last folder row under the pointer, valid or not: a refused drop still needs to say why.
  const hoveredFolderId = useRef<string | null>(null);

  const recordUndo = useCallback((step: UndoStep) => {
    undoStack.current = pushUndoStep(undoStack.current, step);
    setUndoCount(undoStack.current.length);
  }, []);
  const clearUndo = useCallback(() => {
    undoStack.current = [];
    setUndoCount(0);
  }, []);
  const recordFolderRename = useCallback(
    (step: { folderId: string; oldName: string; newName: string }) => recordUndo({ kind: 'renameFolder', ...step }),
    [recordUndo],
  );

  const onDragFileStart = useCallback((id: string) => {
    setDraggedFileId(id);
    setDraggedFileIds(selectedIds?.has(id) ? [...selectedIds] : [id]);
  }, [selectedIds]);
  // A click without hovering over another row moves nothing. Folders that
  // cannot be moved are still draggable, so dropping them explains the refusal.
  const onDragFolderStart = useCallback((id: string) => {
    if (!folders.some(folder => folder.id === id)) return;
    hoveredFolderId.current = null;
    setDraggedFolderId(id);
  }, [folders]);
  const dismissMoveToast = useCallback(() => setMoveToast(null), []);

  // The checks are duplicated here for the highlight and the explanation;
  // move_folder in the backend does the binding check.
  const handleFolderMouseEnter = useCallback(
    (id: string) => {
      if (!draggedFileId && !draggedFolderId) return;
      hoveredFolderId.current = id;
      setDragOverCollectionId(null);
      // Only when every dragged file is in this folder already; with a mixed selection
      // the others still move and the ones already there are reported as skipped.
      if (draggedFileId && sharedFolderId(models, draggedFileIds) === id) {
        setDragOverFolderId(null);
        return;
      }
      if (draggedFolderId && folderMoveBlock(draggedFolderId, id, folders, catalogBaseDir)) {
        setDragOverFolderId(null);
        return;
      }
      setDragOverFolderId(id);
    },
    [draggedFileId, draggedFileIds, draggedFolderId, folders, models, catalogBaseDir],
  );

  // Only reset if no other row is the target already: a late mouseleave
  // must not overwrite a newer mouseenter, otherwise a drop over a card
  // lands in the folder hovered last.
  const handleFolderMouseLeave = useCallback((id: string) => {
    if (hoveredFolderId.current === id) hoveredFolderId.current = null;
    setDragOverFolderId((current) => (current === id ? null : current));
  }, []);

  const handleCollectionMouseEnter = useCallback((id: string) => {
    // Adding to a collection stays a one-card action; a multi-card drag only targets folders.
    if (!draggedFileId || draggedFileIds.length > 1) return;
    setDragOverFolderId(null);
    setDragOverCollectionId(id);
  }, [draggedFileId, draggedFileIds]);
  const handleCollectionMouseLeave = useCallback((id: string) => {
    setDragOverCollectionId(current => current === id ? null : current);
  }, []);

  const onCreateFolder = useCallback(
    (parentId: string | null, name: string) =>
      foldersApi
        .createFolder(parentId, name)
        .then(() => refreshFolders())
        .catch((e) => {
          console.error('[folders] creating failed:', e);
          // Show the error next to the folder tree; catalogBackupError would be invisible without an open panel.
          const err = toAppError(e);
          setMoveToast({ from: name, to: err.message, error: true, unexpected: err.unexpected });
        }),
    [refreshFolders],
  );

  // Shared by the drop on a folder and by the "Move to ..." dialog.
  const moveFiles = useCallback(async (ids: string[], folderId: string) => {
    const target = folders.find((f) => f.id === folderId);
    if (!target || ids.length === 0) return;
    const nameOf = (id: string) => models.find((m) => m.id === id)?.name ?? id;
    try {
      const results = await foldersApi.moveFilesToFolder(ids, folderId);
      const summary = summarizeMoveResults(results);
      if (summary.moved > 0) {
        dismissDragDropTip();
        refreshFolders();
        refreshFiles();
      }
      const items = undoItemsFromResults(results);
      if (items.length > 0) recordUndo({ kind: 'moveFiles', items });
      const shown = summary.moved > 0 ? results.filter((r) => r.status === 'moved').map((r) => r.id) : ids;
      const names = shown.slice(0, 2).map(nameOf);
      setMoveToast({ from: names[0], to: target.path, batch: { names, total: ids.length, summary }, canUndo: items.length > 0 });
    } catch (e) {
      console.error('[folders] moving files failed:', e);
      const err = toAppError(e);
      setMoveToast({ from: nameOf(ids[0]), to: err.message, error: true, unexpected: err.unexpected });
    }
  }, [folders, models, recordUndo, refreshFolders, refreshFiles]);

  // Runs the opposite action without checking anything first: the backend
  // knows the real disk state, and a refusal is reported instead of guessed.
  // The step is consumed either way so a failing step cannot block the ones below it.
  const undo = useCallback(async () => {
    if (undoBusy.current) return;
    const step = undoStack.current[undoStack.current.length - 1];
    if (!step) return;
    undoBusy.current = true;
    undoStack.current = undoStack.current.slice(0, -1);
    setUndoCount(undoStack.current.length);
    let outcome: UndoOutcome;
    try {
      if (step.kind === 'moveFiles') {
        let moved = 0;
        let reason: string | undefined;
        for (const [folderId, fileIds] of groupByOldFolder(step.items)) {
          try {
            const results = await foldersApi.moveFilesToFolder(fileIds, folderId);
            for (const result of results) {
              if (result.status === 'moved') moved++;
              else reason ??= result.status === 'skipped' ? 'name_exists' : result.reason;
            }
          } catch (e) {
            console.error('[folders] undo of file move failed:', e);
            reason ??= messageOf(e);
          }
        }
        outcome = { kind: 'files', ok: moved === step.items.length, count: moved, total: step.items.length, reason };
      } else if (step.kind === 'moveFolder') {
        await foldersApi.moveFolder(step.folderId, step.fromParentId);
        outcome = { kind: 'folderMove', ok: true, name: step.name };
      } else {
        await foldersApi.renameFolder(step.folderId, step.oldName);
        outcome = { kind: 'rename', ok: true, name: step.newName, restoredName: step.oldName };
      }
    } catch (e) {
      console.error('[folders] undo failed:', e);
      outcome = {
        kind: step.kind === 'moveFolder' ? 'folderMove' : 'rename', ok: false,
        name: step.kind === 'moveFolder' ? step.name : step.kind === 'renameFolder' ? step.newName : undefined,
        reason: messageOf(e),
      };
    } finally {
      undoBusy.current = false;
    }
    if (outcome.ok || (outcome.count ?? 0) > 0) {
      refreshFolders();
      refreshFiles();
    }
    setMoveToast({ from: outcome.name ?? '', to: '', undo: outcome, error: !outcome.ok });
  }, [refreshFolders, refreshFiles]);

  useEffect(() => {
    if (!draggedFileId) return;
    const handleMouseUp = () => {
      const fileId = draggedFileId;
      const fileIds = draggedFileIds;
      const folderId = dragOverFolderId;
      const collectionId = dragOverCollectionId;
      setDragOverCollectionId(null);
      setDraggedFileId(null);
      setDraggedFileIds([]);
      setDragOverFolderId(null);
      const file = models.find((m) => m.id === fileId);
      if (collectionId && file && collectionTargets) {
        const target = collectionTargets.collections.find(c => c.id === collectionId);
        if (!target) return;
        collectionsApi.listCollectionFiles(collectionId).then(async members => {
          const already = members.some(member => member.id === fileId);
          if (!already) await collectionTargets.addModelToCollection(fileId, collectionId);
          dismissDragDropTip();
          setMoveToast({ from: file.name, to: target.name, collection: already ? 'already' : 'added' });
        }).catch(e => {
          const err = toAppError(e);
          setMoveToast({ from: file.name, to: err.message, error: true, unexpected: err.unexpected });
        });
        return;
      }
      if (!folderId || !file) return;
      void moveFiles(fileIds, folderId);
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [draggedFileId, draggedFileIds, dragOverFolderId, dragOverCollectionId, collectionTargets, models, moveFiles]);

  // Dragging a folder onto a folder. The checks are repeated here so an
  // invalid target never triggers an invoke.
  useEffect(() => {
    if (!draggedFolderId) return;
    const handleMouseUp = () => {
      const folderId = draggedFolderId;
      const targetId = hoveredFolderId.current;
      hoveredFolderId.current = null;
      setDraggedFolderId(null);
      setDragOverFolderId(null);
      if (!targetId) return;
      const folder = folders.find((f) => f.id === folderId);
      const targetFolder = folders.find((f) => f.id === targetId);
      if (!folder || !targetFolder) return;
      const blocked = folderMoveBlock(folderId, targetId, folders, catalogBaseDir);
      if (blocked) {
        setMoveToast({ from: folder.name, to: '', error: true, blocked });
        return;
      }
      const fromParentId = folder.parentId;
      foldersApi
        .moveFolder(folderId, targetId)
        .then(() => {
          // The old parent has to be remembered now: after the reload the folder only knows its new one.
          if (fromParentId) recordUndo({ kind: 'moveFolder', folderId, name: folder.name, fromParentId });
          setMoveToast({ from: folder.name, to: targetFolder.path, canUndo: !!fromParentId });
          refreshFolders();
          refreshFiles();
        })
        .catch((e) => {
          console.error('[folders] moving folder failed:', e);
          const err = toAppError(e);
          setMoveToast({ from: folder.name, to: err.message, error: true, unexpected: err.unexpected });
        });
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [draggedFolderId, folders, refreshFolders, refreshFiles, catalogBaseDir, recordUndo]);

  return {
    draggedFileId,
    draggedFileIds,
    draggedFilesFolderId,
    draggedFolderId,
    dragOverFolderId,
    dragOverCollectionId,
    handleCollectionMouseEnter,
    handleCollectionMouseLeave,
    moveToast,
    dismissMoveToast,
    onDragFileStart,
    onDragFolderStart,
    handleFolderMouseEnter,
    handleFolderMouseLeave,
    onCreateFolder,
    moveFiles,
    undo,
    undoCount,
    clearUndo,
    recordFolderRename,
  };
}
