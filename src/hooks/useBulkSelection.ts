import { useCallback, useMemo, useState } from 'react';
import * as filesApi from '../lib/api/files';
import { canonicalTag } from '../lib/autoTags';
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
  const [addTagMenuOpen, setAddTagMenuOpen] = useState(false);
  const [tagDraft, setTagDraft] = useState('');
  const [removeTagMenuOpen, setRemoveTagMenuOpen] = useState(false);

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
    setAddTagMenuOpen(false);
    setTagDraft('');
    setRemoveTagMenuOpen(false);
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
    // Nur die betroffenen Modelle lokal patchen statt die ganze Liste neu zu
    // laden; das Backend ist durch addToQueue schon aktuell.
    return Promise.all(notYetQueued.map((m) => filesApi.addToQueue(m.id).then((position) => ({ id: m.id, position })))).then(
      (updates) => {
        const positionById = new Map(updates.map((u) => [u.id, u.position]));
        setModels((prev) =>
          prev.map((m) => (positionById.has(m.id) ? { ...m, queuePosition: positionById.get(m.id)! } : m)),
        );
      },
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

  // Einen Tag allen ausgewaehlten Modellen ohne ihn hinzufuegen: ein addTag()
  // pro Datei (Auswahlen sind klein), danach ein lokaler Patch.
  const bulkAddTagAction = useCallback(() => {
    const tag = canonicalTag(tagDraft.trim());
    if (!tag) return Promise.resolve();
    const ids = selectedForBulk;
    return Promise.all(Array.from(ids).map((id) => filesApi.addTag(id, tag))).then(() => {
      setModels((prev) =>
        prev.map((m) => (ids.has(m.id) && !m.tags.includes(tag) ? { ...m, tags: [...m.tags, tag] } : m)),
      );
      refreshTags();
      setTagDraft('');
      setAddTagMenuOpen(false);
    });
  }, [tagDraft, selectedForBulk, setModels, refreshTags]);

  // Gegenstueck zu bulkAddTagAction; Modelle ohne den Tag werden uebersprungen.
  const bulkRemoveTagAction = useCallback(
    (tag: string) => {
      const ids = Array.from(selectedForBulk).filter((id) => models.find((m) => m.id === id)?.tags.includes(tag));
      return Promise.all(ids.map((id) => filesApi.removeTag(id, tag))).then(() => {
        setModels((prev) =>
          prev.map((m) => (selectedForBulk.has(m.id) ? { ...m, tags: m.tags.filter((t) => t !== tag) } : m)),
        );
        refreshTags();
        setRemoveTagMenuOpen(false);
      });
    },
    [models, selectedForBulk, setModels, refreshTags],
  );

  // Alle Tags der Auswahl, fuer das "Tag entfernen"-Menue.
  const tagsInSelection = useMemo(() => {
    const set = new Set<string>();
    for (const m of models) {
      if (!selectedForBulk.has(m.id)) continue;
      for (const tag of m.tags) set.add(tag);
    }
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [models, selectedForBulk]);

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
    addTagMenuOpen, setAddTagMenuOpen,
    tagDraft, setTagDraft,
    removeTagMenuOpen, setRemoveTagMenuOpen,
    tagsInSelection,
    toggleBulkSelect,
    selectAllVisible,
    clearBulkSelection,
    bulkDelete,
    bulkAddToQueue,
    bulkSetPrintStatus,
    bulkAddToCollectionAction,
    bulkRemoveFromCollectionAction,
    bulkAddTagAction,
    bulkRemoveTagAction,
  };
}
