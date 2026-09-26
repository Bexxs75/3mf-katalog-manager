import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import * as filesApi from '../lib/api/files';
import * as foldersApi from '../lib/api/folders';
import * as catalogMetaApi from '../lib/api/catalogMeta';
import { canonicalTag } from '../lib/autoTags';
import type { ModelFile, ModelFileSummary, Folder, TagCount, ImportResultDto } from '../types';

// Embeds a slim `ModelFileSummary` into the full `ModelFile` shape so
// all components see the same type. Fields not delivered
// (materials, customImage, sliceInfo, costEstimate, sourceUrl) get
// neutral defaults and are only loaded later via `ensureFullModel()`.
function summaryToModelFile(s: ModelFileSummary): ModelFile {
  return {
    id: s.id,
    name: s.name,
    path: s.path,
    folderId: s.folderId,
    tags: [],
    origin: 'local',
    sync: 'local-only',
    dimensionsMm: s.dimensionsMm,
    volumeCm3: s.volumeCm3,
    objectCount: s.objectCount,
    plateCount: null,
    materials: [],
    fileSizeBytes: s.fileSizeBytes,
    importedAt: s.importedAt,
    printStatus: s.printStatus,
    estimatedWeightG: null,
    weightSource: 'estimated',
    sliceInfo: null,
    costEstimate: null,
    lastViewedAt: s.lastViewedAt,
    contentHash: s.contentHash,
    creator: s.creator,
    customImage: null,
    thumbnailImage: s.thumbnailImage,
    renderSnapshotImage: s.renderSnapshotImage,
    sourceUrl: null,
    queuePosition: s.queuePosition,
    favorite: s.favorite,
    deletedAt: null,
  };
}

// Prefixes in the "<field>:<id>" scheme of the mutation counters, mapped to
// ModelFile field names. ensureFullModel() keeps the current value for these
// fields if a mutation ran during its fetch.
const MUTATION_TRACKED_FIELDS: { prefix: string; field: keyof ModelFile }[] = [
  { prefix: 'lastViewedAt', field: 'lastViewedAt' },
  { prefix: 'tags', field: 'tags' },
  { prefix: 'printStatus', field: 'printStatus' },
  { prefix: 'favorite', field: 'favorite' },
  { prefix: 'queue', field: 'queuePosition' },
  { prefix: 'renderSnapshot', field: 'renderSnapshotImage' },
  { prefix: 'sourceUrl', field: 'sourceUrl' },
];

export function useCatalogStore() {
  const [models, setModels] = useState<ModelFile[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [trashModels, setTrashModels] = useState<ModelFile[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // IDs whose full data is already loaded; refreshFiles() resets the list
  // back to summaries.
  const [fullyLoadedIds, setFullyLoadedIds] = useState<Set<string>>(new Set());
  const [skippedSnapshotIds, setSkippedSnapshotIds] = useState<Set<string>>(new Set());
  // IDs that already have a snapshot in the DB according to the summary; they
  // don't need a new one.
  const [summaryConfirmedSnapshotIds, setSummaryConfirmedSnapshotIds] = useState<Set<string>>(new Set());
  // Per model ID, so feedback for model A doesn't stay under model B
  // when the user switches the detail page.
  const [rescanFeedback, setRescanFeedback] = useState<
    { fileId: string; status: 'success' | 'error'; message?: string } | null
  >(null);
  // One-shot scroll request after the next commit (see renameFile);
  // App.tsx resets it to null afterwards.
  const [pendingScrollToId, setPendingScrollToId] = useState<string | null>(null);

  // Rollback for failed optimistic mutations: per "<field>:<id>" the number
  // of running backend calls is counted. When it drops to 0, the record is
  // reloaded once via listFilesByIds([id]); once all calls have settled the
  // backend is always right, no matter which call failed. ("Reset to
  // previous" or "only the newest may roll back" is wrong with overlapping
  // failures.)
  //
  // The epoch per field+ID prevents a late, stale resync from overwriting a
  // newer one: the result is only taken if counter and epoch are unchanged
  // since the resync started.
  //
  // Refs instead of state: the counters themselves must not trigger re-renders.
  const pendingMutationCounts = useRef<Record<string, number>>({}).current;
  const mutationEpochs = useRef<Record<string, number>>({}).current;

  function beginMutation(key: string): void {
    pendingMutationCounts[key] = (pendingMutationCounts[key] ?? 0) + 1;
    mutationEpochs[key] = (mutationEpochs[key] ?? 0) + 1;
  }

  // Always call in the mutation's .finally(), whether success or failure.
  async function endMutationAndResyncIfSettled(key: string, id: string): Promise<void> {
    const remaining = (pendingMutationCounts[key] ?? 1) - 1;
    if (remaining > 0) {
      pendingMutationCounts[key] = remaining;
      return;
    }
    delete pendingMutationCounts[key];
    const epochAtResyncStart = mutationEpochs[key];
    try {
      const [fresh] = await filesApi.listFilesByIds([id]);
      // Only take it if no new mutation started during the await and no
      // newer resync has already taken a more current state.
      if (fresh && pendingMutationCounts[key] === undefined && mutationEpochs[key] === epochAtResyncStart) {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, ...fresh } : m)));
      }
    } catch (e) {
      console.error(`[resync] Nachladen von ${key} fehlgeschlagen:`, e);
      // No further rollback; the next refreshFiles() reconciles.
    }
  }

  const pendingSnapshotIds = useMemo(
    () =>
      models
        .filter(
          (m) =>
            m.renderSnapshotImage === null &&
            !skippedSnapshotIds.has(m.id) &&
            !summaryConfirmedSnapshotIds.has(m.id),
        )
        .map((m) => m.id),
    [models, skippedSnapshotIds, summaryConfirmedSnapshotIds],
  );

  const refreshFolders = useCallback(() => foldersApi.listFolders().then(setFolders), []);

  // Load summaries and file-tag mapping in parallel with one query each and
  // merge the tags in on the client (for tag filtering).
  const loadSummariesWithTags = useCallback(() => {
    return Promise.all([filesApi.listFileSummaries(), filesApi.listAllFileTags()]).then(([summaries, tagsByFileId]) => {
      setSummaryConfirmedSnapshotIds(new Set(summaries.filter((s) => s.hasRenderSnapshot).map((s) => s.id)));
      return summaries.map((s) => ({ ...summaryToModelFile(s), tags: tagsByFileId[s.id] ?? [] }));
    });
  }, []);

  // Loads only the slim summaries. Entries already upgraded are simply
  // loaded again on the next ensureFullModel().
  const refreshFiles = useCallback(() => {
    setFullyLoadedIds(new Set());
    return loadSummariesWithTags().then(setModels);
  }, [loadSummariesWithTags]);
  const refreshTags = useCallback(() => catalogMetaApi.listTagCounts().then(setTags), []);
  const refreshTrash = useCallback(() => filesApi.listTrash().then(setTrashModels), []);

  // Loads a model's full data as soon as it is needed
  // (selection or detail page) and replaces the entry in `models`.
  const ensureFullModel = useCallback(
    (id: string) => {
      if (fullyLoadedIds.has(id) || !models.some((m) => m.id === id)) return;
      // Epoch snapshot before the fetch: if a mutation including resync runs in
      // parallel, the then stale fetch result must not overwrite its value.
      const epochSnapshot = MUTATION_TRACKED_FIELDS.map(({ prefix }) => mutationEpochs[`${prefix}:${id}`]);
      filesApi
        .listFilesByIds([id])
        .then((full) => {
          if (full.length === 0) return;
          setModels((prev) =>
            prev.map((m) => {
              if (m.id !== id) return m;
              const merged: ModelFile = { ...full[0] };
              MUTATION_TRACKED_FIELDS.forEach(({ prefix, field }, index) => {
                const key = `${prefix}:${id}`;
                const stillPending = pendingMutationCounts[key] !== undefined;
                const epochAdvanced = mutationEpochs[key] !== epochSnapshot[index];
                if (stillPending || epochAdvanced) {
                  // A mutation ran for this field during the fetch: keep the current
                  // value, take all other fields from `full[0]`.
                  (merged as unknown as Record<string, unknown>)[field] = m[field];
                }
              });
              return merged;
            }),
          );
          setFullyLoadedIds((prev) => new Set(prev).add(id));
        })
        .catch((e) => {
          console.error('[full-model] Nachladen fehlgeschlagen:', e);
        });
    },
    [models, fullyLoadedIds],
  );

  useEffect(() => {
    loadSummariesWithTags().then((mapped) => {
      setModels(mapped);
      setSelectedId((prev) => prev ?? mapped[0]?.id ?? null);
    });
    refreshFolders();
    refreshTags();
    refreshTrash();
    // Mount only - the refreshers themselves are stable (useCallback without deps).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const skipSnapshot = useCallback((id: string) => {
    setSkippedSnapshotIds((prev) => new Set(prev).add(id));
  }, []);

  const selectModel = useCallback(
    (id: string) => {
      setSelectedId(id);
      ensureFullModel(id);
      const now = new Date().toISOString();
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, lastViewedAt: now } : m)));
      const key = `lastViewedAt:${id}`;
      beginMutation(key);
      filesApi
        .markFileViewed(id)
        .catch((e) => {
          console.error('[last-viewed] Aktualisieren fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [ensureFullModel],
  );

  const mergeImported = useCallback(
    (result: ImportResultDto) => {
      if (result.imported.length) {
        setModels((prev) => [...prev, ...result.imported]);
        // Freshly imported files already come with full data.
        setFullyLoadedIds((prev) => {
          const next = new Set(prev);
          result.imported.forEach((m) => next.add(m.id));
          return next;
        });
        setSelectedId(result.imported[result.imported.length - 1].id);
        refreshFolders();
        refreshTags();
      }
    },
    [refreshFolders, refreshTags],
  );

  const applyLocalDeletion = useCallback(
    (deletedIds: string[]) => {
      setModels((prev) => prev.filter((m) => !deletedIds.includes(m.id)));
      setSelectedId((prev) => (prev && deletedIds.includes(prev) ? null : prev));
      refreshFolders();
      refreshTags();
      refreshTrash();
    },
    [refreshFolders, refreshTags, refreshTrash],
  );

  // For callers where delete_files may have silently skipped IDs:
  // reloads authoritatively instead of filtering locally.
  const refetchAfterPartialDelete = useCallback(
    (affectedIds: string[]) => {
      refreshFiles();
      setSelectedId((prev) => (prev && affectedIds.includes(prev) ? null : prev));
      refreshFolders();
      refreshTags();
      refreshTrash();
    },
    [refreshFiles, refreshFolders, refreshTags, refreshTrash],
  );

  const restoreModel = useCallback(
    (id: string) =>
      filesApi.restoreFile(id).then(() => {
        setTrashModels((prev) => prev.filter((m) => m.id !== id));
        setSelectedId((prev) => (prev === id ? null : prev));
        refreshFiles();
        refreshFolders();
        refreshTags();
      }),
    [refreshFiles, refreshFolders, refreshTags],
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
    (id: string, rawTag: string) => {
      // Map translated names of automatic tags (e.g. "Multipart") to the
      // identifier before updating optimistically - otherwise a second tag
      // flashes briefly until the backend has normalized it.
      const tag = canonicalTag(rawTag);
      const current = models.find((m) => m.id === id);
      if (!current || current.tags.includes(tag)) return;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: [...m.tags, tag] } : m)));
      const key = `tags:${id}`;
      beginMutation(key);
      filesApi
        .addTag(id, tag)
        .then(refreshTags)
        .catch((e) => {
          console.error('[tags] Hinzufügen fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [models, refreshTags],
  );

  // Deliberately NOT optimistic: the UI should be able to show a name
  // collision directly. The error goes to the caller as a rejection.
  const renameFile = useCallback((id: string, name: string) => {
    return filesApi.renameFile(id, name).then(() => {
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, name } : m)));
      // The new name can change the sort position. Scrolling happens in a
      // useEffect in App.tsx, which reliably runs after the commit
      // (requestAnimationFrame here wouldn't have that guarantee).
      setPendingScrollToId(id);
    });
  }, []);

  const removeTag = useCallback(
    (id: string, tag: string) => {
      const current = models.find((m) => m.id === id);
      if (!current) return;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: m.tags.filter((t) => t !== tag) } : m)));
      const key = `tags:${id}`;
      beginMutation(key);
      filesApi
        .removeTag(id, tag)
        .then(refreshTags)
        .catch((e) => {
          console.error('[tags] Entfernen fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
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
      const key = `printStatus:${id}`;
      beginMutation(key);
      filesApi
        .setPrintStatus(id, next)
        .catch((e) => {
          console.error('[print-status] Aktualisieren fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
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
      const key = `favorite:${id}`;
      beginMutation(key);
      filesApi
        .setFavorite(id, next)
        .catch((e) => {
          console.error('[favorite] Aktualisieren fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
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
    const key = `queue:${id}`;
    beginMutation(key);
    filesApi
      .removeFromQueue(id)
      .catch((e) => {
        console.error('[queue] Entfernen fehlgeschlagen:', e);
      })
      .finally(() => {
        void endMutationAndResyncIfSettled(key, id);
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
      // A reorder affects several models; on failure a full refreshFiles()
      // is enough instead of the field+ID counters.
      filesApi.reorderQueue(updates).catch((e) => {
        console.error('[queue] Neusortierung fehlgeschlagen:', e);
        void refreshFiles();
      });
    },
    [models, refreshFiles],
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
    const key = `renderSnapshot:${id}`;
    beginMutation(key);
    filesApi
      .setRenderSnapshot(id, base64)
      .catch((e) => {
        console.error('[render-snapshot] Speichern fehlgeschlagen:', e);
      })
      .finally(() => {
        void endMutationAndResyncIfSettled(key, id);
      });
  }, []);

  const setModelSourceUrl = useCallback((id: string, url: string | null) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, sourceUrl: url } : m)));
    const key = `sourceUrl:${id}`;
    beginMutation(key);
    filesApi
      .setSourceUrl(id, url)
      .catch((e) => {
        console.error('[source-url] Speichern fehlgeschlagen:', e);
      })
      .finally(() => {
        void endMutationAndResyncIfSettled(key, id);
      });
  }, []);

  const rescanMetadata = useCallback((id: string) => {
    setRescanFeedback(null);
    filesApi
      .rescanFileMetadata(id)
      .then((updated) => {
        setModels((prev) => prev.map((m) => (m.id === id ? updated : m)));
        // rescan_file_metadata already returns the full ModelFile record.
        setFullyLoadedIds((prev) => new Set(prev).add(id));
        setRescanFeedback({ fileId: id, status: 'success' });
      })
      .catch((e) => {
        console.error('[rescan] Neu-Einlesen fehlgeschlagen:', e);
        setRescanFeedback({ fileId: id, status: 'error', message: String(e) });
      });
  }, []);

  return {
    models, setModels,
    folders, tags, trashModels,
    selectedId, setSelectedId,
    skippedSnapshotIds, skipSnapshot, pendingSnapshotIds,
    rescanFeedback,
    pendingScrollToId, setPendingScrollToId,
    refreshFolders, refreshFiles, refreshTags, refreshTrash,
    selectModel, mergeImported, ensureFullModel,
    restoreModel, deleteModelPermanently, emptyTrashAction,
    addTag, removeTag, renameFile, deleteModel,
    togglePrintStatus, toggleFavorite,
    addToQueue, removeFromQueue, reorderQueue,
    uploadCustomImage, captureRenderSnapshot, setModelSourceUrl,
    rescanMetadata,
    applyLocalDeletion, refetchAfterPartialDelete,
  };
}
