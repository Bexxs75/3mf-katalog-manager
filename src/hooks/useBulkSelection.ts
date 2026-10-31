import { fileOpsBatch } from '../lib/fileOps';
import { metadataBatch } from '../lib/metadataBatch';
import { canEditMetadata, canManageFiles } from '../lib/webSession';
import { isReadOnly } from '../lib/readOnly';
import { useCallback, useEffect, useMemo, useState } from 'react';
import * as filesApi from '../lib/api/files';
import { canonicalTag } from '../lib/autoTags';
import type { ModelFile } from '../types';

interface UseBulkSelectionArgs {
  models: ModelFile[];
  setModels: React.Dispatch<React.SetStateAction<ModelFile[]>>;
  // Resolves with the reloaded list when the caller can provide it; used to tell which files survived.
  refetchAfterPartialDelete: (ids: string[]) => void | Promise<void | readonly { id: string }[]>;
  refreshFolders: () => void;
  refreshTags: () => void | Promise<void>;
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
  const [deleteFailures, setDeleteFailures] = useState<string[]>([]);
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

  const runWebBatch = (ids: string[], action: (id: string) => Promise<unknown>) => metadataBatch(ids, action, async () => {
    const fresh = await filesApi.listFilesByIds(ids);
    const byId = new Map(fresh.map(model => [model.id, model]));
    setModels(previous => previous.map(model => byId.get(model.id) ?? model));
    await refreshTags();
  });

  const toggleBulkSelect = useCallback((id: string) => {
    if (!canEditMetadata()) return Promise.resolve();
    setSelectedForBulk((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      if (next.size === 0) setConfirmBulkDelete(false);
      return next;
    });
  }, []);

  const selectAllVisible = useCallback((visibleIds: string[]) => !canEditMetadata() ? Promise.resolve() : setSelectedForBulk(new Set(visibleIds)), []);

  const clearBulkSelection = useCallback(() => {
    setSelectedForBulk(new Set());
    setConfirmBulkDelete(false);
    setOpenMenu(null);
    setTagDraft('');
  }, []);

  const deselectBulk = useCallback((ids: string[]) => {
    setSelectedForBulk(previous => {
      if (!ids.some(id => previous.has(id))) return previous;
      const next = new Set([...previous].filter(id => !ids.includes(id)));
      if (next.size === 0) setConfirmBulkDelete(false);
      return next;
    });
  }, []);

  useEffect(() => {
    const activeIds = new Set(models.map(model => model.id));
    setSelectedForBulk(previous => {
      const remaining = new Set([...previous].filter(id => activeIds.has(id)));
      return remaining.size === previous.size ? previous : remaining;
    });
  }, [models]);

  const dismissDeleteFailures = useCallback(() => setDeleteFailures([]), []);

  const bulkDelete = useCallback(async () => {
    if (!canManageFiles()) return Promise.resolve();
    const ids = Array.from(selectedForBulk);
    const nameById = new Map(models.filter(model => selectedForBulk.has(model.id)).map(model => [model.id, model.name]));
    setDeleteFailures([]);
    let deleteError: unknown;
    let deleteFailed = false;
    try {
      if (isReadOnly()) await fileOpsBatch('delete_files', ids);
      else await filesApi.deleteFiles(ids);
    } catch (error) {
      deleteFailed = true;
      deleteError = error;
    }
    // The backend skips files it cannot move (locked, read-only) without an
    // error, so only the reloaded list shows what is still there. The reload
    // runs even after a failed batch and its own failure must not replace the
    // delete error or the list of files that survived.
    let refreshFailed = false;
    let refreshError: unknown;
    let remaining: readonly { id: string }[] | void = undefined;
    try {
      remaining = await refetchAfterPartialDelete(ids);
    } catch (error) {
      refreshFailed = true;
      refreshError = error;
    }
    setConfirmBulkDelete(false);
    if (remaining) {
      const stillThere = new Set(remaining.map(model => model.id));
      setDeleteFailures(ids.filter(id => stillThere.has(id)).map(id => nameById.get(id) ?? id));
    }
    if (deleteFailed) throw deleteError;
    if (refreshFailed) throw refreshError;
  }, [models, selectedForBulk, refetchAfterPartialDelete]);

  const bulkAddToQueue = useCallback(() => {
    if (!canEditMetadata()) return Promise.resolve();
    const notYetQueued = models.filter((m) => selectedForBulk.has(m.id) && m.queuePosition === null);
    if (isReadOnly()) return runWebBatch(notYetQueued.map(m => m.id), filesApi.addToQueue);
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
      !canEditMetadata() ? Promise.resolve() : isReadOnly() ? runWebBatch([...selectedForBulk], id => filesApi.setPrintStatus(id, status)) : Promise.all(Array.from(selectedForBulk).map((id) => filesApi.setPrintStatus(id, status))).then(() => {
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
    if (!canEditMetadata()) return Promise.resolve();
    const tag = canonicalTag(value.trim().replace(/^#/, ''));
    if (!tag || tag.length > 100) return Promise.resolve();
    const ids = selectedForBulk;
    if (isReadOnly()) return runWebBatch([...ids], id => filesApi.addTag(id, tag));
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
      if (!canEditMetadata()) return Promise.resolve();
      const ids = Array.from(selectedForBulk).filter((id) => models.find((m) => m.id === id)?.tags.includes(tag));
      if (isReadOnly()) return runWebBatch(ids, id => filesApi.removeTag(id, tag));
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
      !canEditMetadata() ? Promise.resolve() : bulkAddToCollection(Array.from(selectedForBulk), collectionId).then(() => clearBulkSelection()),
    [bulkAddToCollection, selectedForBulk, clearBulkSelection],
  );

  const bulkRemoveFromCollectionAction = useCallback(
    () => !canEditMetadata() ? Promise.resolve() : bulkRemoveFromCollection(Array.from(selectedForBulk)).then(() => clearBulkSelection()),
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
    deselectBulk,
    bulkDelete,
    deleteFailures,
    dismissDeleteFailures,
    bulkAddToQueue,
    bulkSetPrintStatus,
    bulkAddToCollectionAction,
    bulkRemoveFromCollectionAction,
    bulkAddTagAction,
    bulkRemoveTagAction,
  };
}
