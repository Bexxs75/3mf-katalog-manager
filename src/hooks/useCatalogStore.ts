import { useCallback, useEffect, useMemo, useState } from 'react';
import * as filesApi from '../lib/api/files';
import * as foldersApi from '../lib/api/folders';
import * as catalogMetaApi from '../lib/api/catalogMeta';
import type { ModelFile, Folder, TagCount, CreatorCount, SavedFilter, ImportResultDto } from '../types';

export function useCatalogStore() {
  const [models, setModels] = useState<ModelFile[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [creators, setCreators] = useState<CreatorCount[]>([]);
  const [savedFilters, setSavedFilters] = useState<SavedFilter[]>([]);
  const [trashModels, setTrashModels] = useState<ModelFile[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [skippedSnapshotIds, setSkippedSnapshotIds] = useState<Set<string>>(new Set());
  const [rescanFeedback, setRescanFeedback] = useState<
    { fileId: string; status: 'success' | 'error'; message?: string } | null
  >(null);

  const pendingSnapshotIds = useMemo(
    () => models.filter((m) => m.renderSnapshotImage === null && !skippedSnapshotIds.has(m.id)).map((m) => m.id),
    [models, skippedSnapshotIds],
  );

  const refreshFolders = useCallback(() => foldersApi.listFolders().then(setFolders), []);
  const refreshFiles = useCallback(() => filesApi.listFiles().then(setModels), []);
  const refreshTags = useCallback(() => catalogMetaApi.listTagCounts().then(setTags), []);
  const refreshCreators = useCallback(() => catalogMetaApi.listCreators().then(setCreators), []);
  const refreshSavedFilters = useCallback(() => catalogMetaApi.listSavedFilters().then(setSavedFilters), []);
  const refreshTrash = useCallback(() => filesApi.listTrash().then(setTrashModels), []);

  useEffect(() => {
    filesApi.listFiles().then((files) => {
      setModels(files);
      setSelectedId((prev) => prev ?? files[0]?.id ?? null);
    });
    refreshFolders();
    refreshTags();
    refreshCreators();
    refreshSavedFilters();
    refreshTrash();
    // Nur beim Mount - die Refresher selbst sind stabil (useCallback ohne Deps).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const skipSnapshot = useCallback((id: string) => {
    setSkippedSnapshotIds((prev) => new Set(prev).add(id));
  }, []);

  const selectModel = useCallback((id: string) => {
    setSelectedId(id);
    const now = new Date().toISOString();
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, lastViewedAt: now } : m)));
    filesApi.markFileViewed(id).catch((e) => {
      console.error('[last-viewed] Aktualisieren fehlgeschlagen:', e);
    });
  }, []);

  const mergeImported = useCallback(
    (result: ImportResultDto) => {
      if (result.imported.length) {
        setModels((prev) => [...prev, ...result.imported]);
        setSelectedId(result.imported[result.imported.length - 1].id);
        refreshFolders();
        refreshTags();
        refreshCreators();
      }
    },
    [refreshFolders, refreshTags, refreshCreators],
  );

  const applyLocalDeletion = useCallback(
    (deletedIds: string[]) => {
      setModels((prev) => prev.filter((m) => !deletedIds.includes(m.id)));
      setSelectedId((prev) => (prev && deletedIds.includes(prev) ? null : prev));
      refreshFolders();
      refreshTags();
      refreshCreators();
      refreshTrash();
    },
    [refreshFolders, refreshTags, refreshCreators, refreshTrash],
  );

  // Fuer Aufrufer, bei denen delete_files einzelne IDs stillschweigend
  // uebersprungen haben kann (siehe deleteSelectedCleanupFiles) - laedt
  // die Modell-Liste autoritativ neu statt lokal zu filtern.
  const refetchAfterPartialDelete = useCallback(
    (affectedIds: string[]) => {
      filesApi.listFiles().then(setModels);
      setSelectedId((prev) => (prev && affectedIds.includes(prev) ? null : prev));
      refreshFolders();
      refreshTags();
      refreshCreators();
      refreshTrash();
    },
    [refreshFolders, refreshTags, refreshCreators, refreshTrash],
  );

  const restoreModel = useCallback(
    (id: string) =>
      filesApi.restoreFile(id).then(() => {
        setTrashModels((prev) => prev.filter((m) => m.id !== id));
        setSelectedId((prev) => (prev === id ? null : prev));
        refreshFiles();
        refreshFolders();
        refreshTags();
        refreshCreators();
      }),
    [refreshFiles, refreshFolders, refreshTags, refreshCreators],
  );

  const deleteModelPermanently = useCallback(
    (id: string) =>
      filesApi.deleteFilePermanently(id).then(() => {
        setTrashModels((prev) => prev.filter((m) => m.id !== id));
        setSelectedId((prev) => (prev === id ? null : prev));
      }),
    [],
  );

  const emptyTrashAction = useCallback(() => filesApi.emptyTrash().then(() => setTrashModels([])), []);

  const addTag = useCallback(
    (id: string, tag: string) => {
      const current = models.find((m) => m.id === id);
      if (!current || current.tags.includes(tag)) return;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: [...m.tags, tag] } : m)));
      filesApi.addTag(id, tag).then(refreshTags);
    },
    [models, refreshTags],
  );

  const removeTag = useCallback(
    (id: string, tag: string) => {
      const current = models.find((m) => m.id === id);
      if (!current) return;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: m.tags.filter((t) => t !== tag) } : m)));
      filesApi.removeTag(id, tag).then(refreshTags);
    },
    [models, refreshTags],
  );

  const deleteModel = useCallback(
    (id: string) => filesApi.deleteFile(id).then(() => applyLocalDeletion([id])),
    [applyLocalDeletion],
  );

  const togglePrintStatus = useCallback(
    (id: string) => {
      const current = models.find((m) => m.id === id);
      if (!current) return;
      const next = current.printStatus === 'printed' ? 'not_printed' : 'printed';
      setModels((prev) =>
        prev.map((m) =>
          m.id === id ? { ...m, printStatus: next, queuePosition: next === 'printed' ? null : m.queuePosition } : m,
        ),
      );
      filesApi.setPrintStatus(id, next).catch((e) => {
        console.error('[print-status] Aktualisieren fehlgeschlagen:', e);
      });
    },
    [models],
  );

  const toggleFavorite = useCallback(
    (id: string) => {
      const current = models.find((m) => m.id === id);
      if (!current) return;
      const next = !current.favorite;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, favorite: next } : m)));
      filesApi.setFavorite(id, next).catch((e) => {
        console.error('[favorite] Aktualisieren fehlgeschlagen:', e);
      });
    },
    [models],
  );

  const addToQueue = useCallback((id: string) => {
    filesApi
      .addToQueue(id)
      .then((position) => {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: position } : m)));
      })
      .catch((e) => console.error('[queue] Hinzufügen fehlgeschlagen:', e));
  }, []);

  const removeFromQueue = useCallback((id: string) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: null } : m)));
    filesApi.removeFromQueue(id).catch((e) => {
      console.error('[queue] Entfernen fehlgeschlagen:', e);
    });
  }, []);

  const reorderQueue = useCallback(
    (orderedIds: string[]) => {
      const updates: { fileId: string; position: number }[] = [];
      orderedIds.forEach((id, index) => {
        const position = index + 1;
        const current = models.find((m) => m.id === id);
        if (current && current.queuePosition !== position) {
          updates.push({ fileId: id, position });
        }
      });
      if (updates.length === 0) return;
      setModels((prev) =>
        prev.map((m) => {
          const index = orderedIds.indexOf(m.id);
          return index === -1 ? m : { ...m, queuePosition: index + 1 };
        }),
      );
      filesApi.reorderQueue(updates).catch((e) => {
        console.error('[queue] Neusortierung fehlgeschlagen:', e);
      });
    },
    [models],
  );

  const uploadCustomImage = useCallback((id: string) => {
    filesApi
      .uploadCustomImage(id)
      .then((customImage) => {
        if (customImage === null) return;
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, customImage } : m)));
      })
      .catch((e) => {
        console.error('[custom-image] Hochladen fehlgeschlagen:', e);
      });
  }, []);

  const captureRenderSnapshot = useCallback((id: string, base64: string) => {
    const renderSnapshotImage = `data:image/png;base64,${base64}`;
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, renderSnapshotImage } : m)));
    filesApi.setRenderSnapshot(id, base64).catch((e) => {
      console.error('[render-snapshot] Speichern fehlgeschlagen:', e);
    });
  }, []);

  const setModelSourceUrl = useCallback((id: string, url: string | null) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, sourceUrl: url } : m)));
    filesApi.setSourceUrl(id, url).catch((e) => {
      console.error('[source-url] Speichern fehlgeschlagen:', e);
    });
  }, []);

  const rescanMetadata = useCallback((id: string) => {
    setRescanFeedback(null);
    filesApi
      .rescanFileMetadata(id)
      .then((updated) => {
        setModels((prev) => prev.map((m) => (m.id === id ? updated : m)));
        setRescanFeedback({ fileId: id, status: 'success' });
      })
      .catch((e) => {
        console.error('[rescan] Neu-Einlesen fehlgeschlagen:', e);
        setRescanFeedback({ fileId: id, status: 'error', message: String(e) });
      });
  }, []);

  return {
    models, setModels,
    folders, tags, creators, savedFilters, trashModels,
    selectedId, setSelectedId,
    skippedSnapshotIds, skipSnapshot, pendingSnapshotIds,
    rescanFeedback,
    refreshFolders, refreshFiles, refreshTags, refreshCreators, refreshTrash,
    selectModel, mergeImported,
    restoreModel, deleteModelPermanently, emptyTrashAction,
    addTag, removeTag, deleteModel,
    togglePrintStatus, toggleFavorite,
    addToQueue, removeFromQueue, reorderQueue,
    uploadCustomImage, captureRenderSnapshot, setModelSourceUrl,
    rescanMetadata,
    applyLocalDeletion, refetchAfterPartialDelete,
  };
}
