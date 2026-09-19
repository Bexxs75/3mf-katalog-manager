import { useCallback, useState } from 'react';
import * as filesApi from '../lib/api/files';
import type { ModelFile } from '../types';

interface UseBulkSelectionArgs {
  models: ModelFile[];
  setModels: React.Dispatch<React.SetStateAction<ModelFile[]>>;
  refreshFolders: () => void;
  refreshTags: () => void;
  refreshCreators: () => void;
  refreshTrash: () => void;
  bulkAddToCollection: (fileIds: string[], collectionId: string) => Promise<void>;
  bulkRemoveFromCollection: (fileIds: string[]) => Promise<void>;
}

export function useBulkSelection({
  models,
  setModels,
  refreshFolders,
  refreshTags,
  refreshCreators,
  refreshTrash,
  bulkAddToCollection,
  bulkRemoveFromCollection,
}: UseBulkSelectionArgs) {
  const [selectedForBulk, setSelectedForBulk] = useState<Set<string>>(new Set());
  const [confirmBulkDelete, setConfirmBulkDelete] = useState(false);
  const [addToCollectionMenuOpen, setAddToCollectionMenuOpen] = useState(false);

  const toggleBulkSelect = useCallback((id: string) => {
    setSelectedForBulk((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      if (next.size === 0) setConfirmBulkDelete(false);
      return next;
    });
  }, []);

  const selectAllVisible = useCallback((visibleIds: string[]) => setSelectedForBulk(new Set(visibleIds)), []);

  const clearBulkSelection = useCallback(() => {
    setSelectedForBulk(new Set());
    setConfirmBulkDelete(false);
  }, []);

  const bulkDelete = useCallback(() => {
    const ids = Array.from(selectedForBulk);
    return filesApi.deleteFiles(ids).then(() => {
      setModels((prev) => prev.filter((m) => !selectedForBulk.has(m.id)));
      clearBulkSelection();
      refreshFolders();
      refreshTags();
      refreshCreators();
      refreshTrash();
    });
  }, [selectedForBulk, setModels, clearBulkSelection, refreshFolders, refreshTags, refreshCreators, refreshTrash]);

  const bulkAddToQueue = useCallback(() => {
    const notYetQueued = models.filter((m) => selectedForBulk.has(m.id) && m.queuePosition === null);
    return Promise.all(notYetQueued.map((m) => filesApi.addToQueue(m.id))).then(() =>
      filesApi.listFiles().then(setModels),
    );
  }, [models, selectedForBulk, setModels]);

  const bulkSetPrintStatus = useCallback(
    (status: 'printed' | 'not_printed') =>
      Promise.all(Array.from(selectedForBulk).map((id) => filesApi.setPrintStatus(id, status))).then(() => {
        setModels((prev) =>
          prev.map((m) =>
            selectedForBulk.has(m.id)
              ? { ...m, printStatus: status, queuePosition: status === 'printed' ? null : m.queuePosition }
              : m,
          ),
        );
      }),
    [selectedForBulk, setModels],
  );

  const bulkAddToCollectionAction = useCallback(
    (collectionId: string) =>
      bulkAddToCollection(Array.from(selectedForBulk), collectionId).then(() => clearBulkSelection()),
    [bulkAddToCollection, selectedForBulk, clearBulkSelection],
  );

  const bulkRemoveFromCollectionAction = useCallback(
    () => bulkRemoveFromCollection(Array.from(selectedForBulk)).then(() => clearBulkSelection()),
    [bulkRemoveFromCollection, selectedForBulk, clearBulkSelection],
  );

  return {
    selectedForBulk,
    confirmBulkDelete, setConfirmBulkDelete,
    addToCollectionMenuOpen, setAddToCollectionMenuOpen,
    toggleBulkSelect,
    selectAllVisible,
    clearBulkSelection,
    bulkDelete,
    bulkAddToQueue,
    bulkSetPrintStatus,
    bulkAddToCollectionAction,
    bulkRemoveFromCollectionAction,
  };
}
