import { isReadOnly } from '../lib/readOnly';
import { useCallback, useEffect, useMemo, useState } from 'react';
import * as filesApi from '../lib/api/files';
import { canonicalTag } from '../lib/autoTags';
import type { ModelFile } from '../types';

interface UseBulkSelectionArgs {
  models: ModelFile[];
  setModels: React.Dispatch<React.SetStateAction<ModelFile[]>>;
  refetchAfterPartialDelete: (ids: string[]) => void | Promise<void>;
  refreshFolders: () => void;
  refreshTags: () => void;
  refreshTrash: () => void;
  bulkAddToCollection: (fileIds: string[], collectionId: string) => Promise<void>;
  bulkRemoveFromCollection: (fileIds: string[]) => Promise<void>;
}

export function useBulkSelection({
  models,
  setModels,
  refetchAfterPartialDelete,
  refreshTags,
  bulkAddToCollection,
  bulkRemoveFromCollection,
}: UseBulkSelectionArgs) {
  const [selectedForBulk, setSelectedForBulk] = useState<Set<string>>(new Set());
  const [confirmBulkDelete, setConfirmBulkDelete] = useState(false);
  // A single menu state prevents overlapping menus for every caller.
  const [openMenu, setOpenMenu] = useState<'collection' | 'addTag' | 'removeTag' | null>(null);
  const addToCollectionMenuOpen = openMenu === 'collection';
  const addTagMenuOpen = openMenu === 'addTag';
  const removeTagMenuOpen = openMenu === 'removeTag';
  const setAddToCollectionMenuOpen = useCallback((value: boolean) => {
    setOpenMenu((current) => value ? 'collection' : current === 'collection' ? null : current);
  }, []);
  const setAddTagMenuOpen = useCallback((value: boolean) => {
    setOpenMenu((current) => value ? 'addTag' : current === 'addTag' ? null : current);
  }, []);
  const setRemoveTagMenuOpen = useCallback((value: boolean) => {
    setOpenMenu((current) => value ? 'removeTag' : current === 'removeTag' ? null : current);
  }, []);
  const [tagDraft, setTagDraft] = useState('');

  const toggleBulkSelect = useCallback((id: string) => {
    if (isReadOnly()) return Promise.resolve();
    setSelectedForBulk((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      if (next.size === 0) setConfirmBulkDelete(false);
      return next;
    });
  }, []);

  const selectAllVisible = useCallback((visibleIds: string[]) => isReadOnly() ? Promise.resolve() : setSelectedForBulk(new Set(visibleIds)), []);

  const clearBulkSelection = useCallback(() => {
    setSelectedForBulk(new Set());
    setConfirmBulkDelete(false);
    setOpenMenu(null);
    setTagDraft('');
  }, []);

  useEffect(() => {
    const activeIds = new Set(models.map(model => model.id));
    setSelectedForBulk(previous => {
      const remaining = new Set([...previous].filter(id => activeIds.has(id)));
      return remaining.size === previous.size ? previous : remaining;
    });
  }, [models]);

  const bulkDelete = useCallback(async () => {
    if (isReadOnly()) return Promise.resolve();
    const ids = Array.from(selectedForBulk);
    try {
      await filesApi.deleteFiles(ids);
    } finally {
      // A successful batch can still contain skipped files; only the backend knows what remains.
      await refetchAfterPartialDelete(ids);
      setConfirmBulkDelete(false);
    }
  }, [selectedForBulk, refetchAfterPartialDelete]);

  const bulkAddToQueue = useCallback(() => {
    if (isReadOnly()) return Promise.resolve();
    const notYetQueued = models.filter((m) => selectedForBulk.has(m.id) && m.queuePosition === null);
    // Patch only the affected models locally instead of reloading the whole
    // list; the backend is already up to date through addToQueue.
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
      isReadOnly() ? Promise.resolve() : Promise.all(Array.from(selectedForBulk).map((id) => filesApi.setPrintStatus(id, status))).then(() => {
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

  // Add a tag to all selected models that don't have it: one addTag()
  // per file (selections are small), then one local patch.
  const bulkAddTagAction = useCallback((value = tagDraft) => {
    if (isReadOnly()) return Promise.resolve();
    const tag = canonicalTag(value.trim().replace(/^#/, ''));
    if (!tag || tag.length > 100) return Promise.resolve();
    const ids = selectedForBulk;
    return Promise.all(Array.from(ids).map((id) => filesApi.addTag(id, tag))).then(() => {
      setModels((prev) =>
        prev.map((m) => (ids.has(m.id) && !m.tags.includes(tag) ? { ...m, tags: [...m.tags, tag] } : m)),
      );
      refreshTags();
      setTagDraft('');
      setAddTagMenuOpen(false);
    });
  }, [tagDraft, selectedForBulk, setModels, refreshTags, setAddTagMenuOpen]);

  // Counterpart to bulkAddTagAction; models without the tag are skipped.
  const bulkRemoveTagAction = useCallback(
    (tag: string) => {
      if (isReadOnly()) return Promise.resolve();
      const ids = Array.from(selectedForBulk).filter((id) => models.find((m) => m.id === id)?.tags.includes(tag));
      return Promise.all(ids.map((id) => filesApi.removeTag(id, tag))).then(() => {
        setModels((prev) =>
          prev.map((m) => (selectedForBulk.has(m.id) ? { ...m, tags: m.tags.filter((t) => t !== tag) } : m)),
        );
        refreshTags();
        setRemoveTagMenuOpen(false);
      });
    },
    [models, selectedForBulk, setModels, refreshTags, setRemoveTagMenuOpen],
  );

  // All tags of the selection, for the "Remove tag" menu.
  const tagsInSelection = useMemo(() => {
    const set = new Set<string>();
    for (const m of models) {
      if (!selectedForBulk.has(m.id)) continue;
      for (const tag of m.tags) set.add(tag);
    }
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [models, selectedForBulk]);

  const tagsInAllSelection = useMemo(() => {
    const selectedModels = models.filter(model => selectedForBulk.has(model.id));
    if (selectedModels.length === 0) return [];
    const assigned = selectedModels.map(model => new Set(model.tags.map(canonicalTag)));
    return [...assigned[0]].filter(tag => assigned.every(tags => tags.has(tag)));
  }, [models, selectedForBulk]);

  const bulkAddToCollectionAction = useCallback(
    (collectionId: string) =>
      isReadOnly() ? Promise.resolve() : bulkAddToCollection(Array.from(selectedForBulk), collectionId).then(() => clearBulkSelection()),
    [bulkAddToCollection, selectedForBulk, clearBulkSelection],
  );

  const bulkRemoveFromCollectionAction = useCallback(
    () => isReadOnly() ? Promise.resolve() : bulkRemoveFromCollection(Array.from(selectedForBulk)).then(() => clearBulkSelection()),
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
    tagsInAllSelection,
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
