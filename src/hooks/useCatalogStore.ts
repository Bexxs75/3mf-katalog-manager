import { webFileNameParts } from '../lib/webFileName';
import { fileOp, fileOpsBatch, emptyWebTrash, type TrashSummary } from '../lib/fileOps';
import { metadataBatch } from '../lib/metadataBatch';
import { reportMetadataError } from '../lib/metadataErrors';
import { canEditMetadata, canSaveSnapshots, canManageFiles } from '../lib/webSession';
import { isReadOnly } from '../lib/readOnly';
import { hasInterruptedPreviewAttempt } from '../lib/snapshotQueue';
import { isStepFilePath } from '../lib/stepFile';
import { clearModelImages, invalidateModelImages } from './useModelImages';
import { readImageFile } from '../lib/printLogPhoto';

/** The server's answer to a repeated write whose stored reply was too large to keep. */
const isLostAnswer = (e: unknown) => typeof e === 'object' && e !== null
  && ((e as { message?: unknown }).message === 'request_id_result_unavailable' || (e as { kind?: unknown }).kind === 'request_id_result_unavailable');
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import * as filesApi from '../lib/api/files';
import * as foldersApi from '../lib/api/folders';
import * as catalogMetaApi from '../lib/api/catalogMeta';
import { canonicalTag } from '../lib/autoTags';
import { toAppError } from '../lib/errors';
import type { ModelFile, ModelFileSummary, Folder, TagCount, ImportResultDto } from '../types';

// Embeds a slim `ModelFileSummary` into the full `ModelFile` shape so
// all components see the same type. Fields not delivered
// (customImage, sliceInfo, costEstimate, sourceUrl) get
// neutral defaults and are only loaded later via `ensureFullModel()`.
function summaryToModelFile(s: ModelFileSummary): ModelFile {
  return {
    id: s.id,
    name: s.name,
    path: s.path,
    fileType: s.fileType,
    folderId: s.folderId,
    tags: [],
    origin: 'local',
    sync: 'local-only',
    dimensionsMm: s.dimensionsMm,
    volumeCm3: s.volumeCm3,
    objectCount: s.objectCount,
    plateCount: null,
    materials: s.materials,
    fileSizeBytes: s.fileSizeBytes,
    importedAt: s.importedAt,
    fileModifiedAt: s.fileModifiedAt,
    printStatus: s.printStatus,
    estimatedWeightG: s.estimatedWeightG,
    weightSource: s.weightSource,
    sliceInfo: null,
    costEstimate: null,
    lastViewedAt: s.lastViewedAt,
    contentHash: s.contentHash,
    creator: s.creator,
    customImage: null,
    thumbnailImage: null,
    renderSnapshotImage: null,
    hasThumbnail: s.hasThumbnail,
    hasRenderSnapshot: s.hasRenderSnapshot,
    sourceUrl: null,
    license: s.license,
    sourcePlatform: s.sourcePlatform,
    sourceModelId: s.sourceModelId,
    sourceProfileId: s.sourceProfileId,
    queuePosition: s.queuePosition,
    favorite: s.favorite,
    deletedAt: null,
    summaryOnly: true,
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
  { prefix: 'renderSnapshot', field: 'hasRenderSnapshot' },
  { prefix: 'sourceUrl', field: 'sourceUrl' },
  { prefix: 'license', field: 'license' },
];

interface CatalogStoreOptions {
  /** Select the first model after loading. Off when the details panel opens only on demand,
   *  otherwise it would appear at every start without anyone having clicked a model. */
  preselectFirst?: boolean;
  detailModelId?: string | null;
}

export function useCatalogStore({ preselectFirst = true, detailModelId = null }: CatalogStoreOptions = {}) {
  const loadGeneration = useRef(0);
  const pendingFullLoads = useRef(new Map<string, number>());
  const [refreshGeneration, setRefreshGeneration] = useState(0);
  const [initialLoading, setInitialLoading] = useState(true);
  const [initialLoadFailed, setInitialLoadFailed] = useState(false);
  const [models, setModels] = useState<ModelFile[]>([]);
  // Read by callbacks that outlive a render (the undo of blocking an automatic tag calls the
  // addTag of the moment the block started): a stale list would still contain the removed tag.
  const modelsRef = useRef(models);
  modelsRef.current = models;
  const [folders, setFolders] = useState<Folder[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [trashModels, setTrashModels] = useState<ModelFile[]>([]);
  const trashModelsRef = useRef(trashModels);
  trashModelsRef.current = trashModels;
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
    { fileId: string; status: 'success' | 'error'; message?: string; unexpected?: boolean } | null
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

  const currentModels = useRef(models);
  currentModels.current = models;
  const rollbackValues = useRef(new Map<string, Partial<ModelFile>>()).current;
  const rollbackQueueEpochs = useRef(new Map<string, number>()).current;
  const failedMutations = useRef(new Set<string>()).current;
  function mutationFailed(key: string, error: unknown) {
    if (isReadOnly()) failedMutations.add(key);
    reportMetadataError(error);
  }
  function rollback(key: string, id: string) {
    const saved = rollbackValues.get(key);
    if (!saved) return;
    const values = { ...saved };
    if (key.startsWith('printStatus:') && (pendingMutationCounts[`queue:${id}`] || (mutationEpochs[`queue:${id}`] ?? 0) !== rollbackQueueEpochs.get(key))) delete values.queuePosition;
    if (values) setModels(previous => previous.map(model => model.id === id ? { ...model, ...values } : model));
  }

  function beginMutation(key: string): void {
    if (isReadOnly() && !pendingMutationCounts[key]) {
      const [prefix, id] = key.split(':');
      const field = MUTATION_TRACKED_FIELDS.find(entry => entry.prefix === prefix)?.field;
      const model = currentModels.current.find(model => model.id === id);
      if (prefix === 'printStatus') rollbackQueueEpochs.set(key, mutationEpochs[`queue:${id}`] ?? 0);
      if (field && model) rollbackValues.set(key, { [field]: model[field], ...(prefix === 'printStatus' ? { queuePosition: model.queuePosition } : {}) });
    }
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
    if (isReadOnly() && failedMutations.has(key)) rollback(key, id);
    const epochAtResyncStart = mutationEpochs[key];
    const generation = loadGeneration.current;
    try {
      const [fresh] = await filesApi.listFilesByIds([id]);
      // Only take it if no new mutation started during the await and no
      // newer resync has already taken a more current state.
      if (fresh && generation === loadGeneration.current && pendingMutationCounts[key] === undefined && mutationEpochs[key] === epochAtResyncStart) {
        const queueUnchanged = !pendingMutationCounts[`queue:${id}`] && (mutationEpochs[`queue:${id}`] ?? 0) === rollbackQueueEpochs.get(key);
        setModels((prev) => prev.map((m) => {
          if (m.id !== id) return m;
          if (!isReadOnly()) return { ...m, ...fresh, summaryOnly: false, hasThumbnail: fresh.thumbnailImage !== null, hasRenderSnapshot: fresh.renderSnapshotImage !== null };
          const field = MUTATION_TRACKED_FIELDS.find(entry => entry.prefix === key.split(':')[0])?.field;
          return field ? { ...m, [field]: fresh[field], ...(field === 'printStatus' && queueUnchanged ? { queuePosition: fresh.queuePosition } : {}) } : m;
        }));
      }
    } catch (e) {
      console.error(`[resync] reloading ${key} failed:`, e); reportMetadataError(e);
      if (isReadOnly() && failedMutations.has(key) && mutationEpochs[key] === epochAtResyncStart && !pendingMutationCounts[key]) rollback(key, id);
    } finally {
      if (mutationEpochs[key] === epochAtResyncStart && !pendingMutationCounts[key]) {
        rollbackValues.delete(key); rollbackQueueEpochs.delete(key); failedMutations.delete(key);
      }
    }
  }

  const pendingSnapshotIds = useMemo(
    () =>
      models
        .filter(
          (m) =>
            !(m.hasRenderSnapshot ?? m.renderSnapshotImage !== null) &&
            !(isStepFilePath(m.path) && hasInterruptedPreviewAttempt(m.id)) &&
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
      return summaries.map((s) => ({ ...summaryToModelFile(s), tags: tagsByFileId[s.id] ?? [] }));
    });
  }, []);

  // Loads only the slim summaries. Entries already upgraded are simply
  // loaded again on the next ensureFullModel().
  const loadFiles = useCallback(() => {
    const generation = ++loadGeneration.current;
    return loadSummariesWithTags().then(mapped => {
      // A superseded load is still the backend's answer for the caller, even though it is not shown.
      if (generation !== loadGeneration.current) return mapped;
      clearModelImages();
      setSummaryConfirmedSnapshotIds(new Set(mapped.filter(model => model.hasRenderSnapshot).map(model => model.id)));
      setFullyLoadedIds(new Set());
      setModels(mapped);
      setInitialLoadFailed(false);
      // Reads begun while summaries were loading are stale too.
      setRefreshGeneration(++loadGeneration.current);
      return mapped;
    });
  }, [loadSummariesWithTags]);
  const refreshFiles = useCallback((): Promise<void> => loadFiles().then(() => undefined), [loadFiles]);
  const refreshTags = useCallback(() => catalogMetaApi.listTagCounts().then(setTags), []);
  const refreshTrash = useCallback(() => filesApi.listTrash().then(setTrashModels), []);

  // Loads a model's full data as soon as it is needed
  // (selection or detail page) and replaces the entry in `models`.
  const ensureFullModel = useCallback(
    (id: string) => {
      if (fullyLoadedIds.has(id) || !models.some((m) => m.id === id)) return;
      const generation = loadGeneration.current;
      if (pendingFullLoads.current.get(id) === generation) return;
      pendingFullLoads.current.set(id, generation);
      // Epoch snapshot before the fetch: if a mutation including resync runs in
      // parallel, the then stale fetch result must not overwrite its value.
      const epochSnapshot = MUTATION_TRACKED_FIELDS.map(({ prefix }) => mutationEpochs[`${prefix}:${id}`]);
      filesApi
        .listFilesByIds([id])
        .then((full) => {
          if (full.length === 0 || generation !== loadGeneration.current) return;
          setModels((prev) =>
            prev.map((m) => {
              if (m.id !== id) return m;
              const merged: ModelFile = { ...full[0], hasRenderSnapshot: full[0].renderSnapshotImage !== null };
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
          console.error('[full-model] loading failed:', e); reportMetadataError(e);
        }).finally(() => {
          if (pendingFullLoads.current.get(id) === generation) pendingFullLoads.current.delete(id);
        });
    },
    [models, fullyLoadedIds],
  );

  useEffect(() => {
    if (detailModelId) ensureFullModel(detailModelId);
    if (refreshGeneration > 0 && selectedId && selectedId !== detailModelId) ensureFullModel(selectedId);
    // Reload visible details when summaries replace full records, even if selection did not change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detailModelId, refreshGeneration, initialLoading]);

  useEffect(() => {
    let active = true;
    const generation = loadGeneration.current;
    loadSummariesWithTags().then((mapped) => {
      if (!active || generation !== loadGeneration.current) return;
      clearModelImages();
      setSummaryConfirmedSnapshotIds(new Set(mapped.filter(model => model.hasRenderSnapshot).map(model => model.id)));
      setModels(mapped);
      if (preselectFirst) {
        const first = mapped.find(m => !(isStepFilePath(m.path) && hasInterruptedPreviewAttempt(m.id)));
        setSelectedId((prev) => prev ?? first?.id ?? null);
      }
    }).catch(e => {
      if (!active) return;
      console.error('[catalog] initial load failed:', e); reportMetadataError(e);
      setInitialLoadFailed(true);
    }).finally(() => { if (active) setInitialLoading(false); });
    refreshFolders();
    refreshTags();
    refreshTrash();
    return () => { active = false; };
    // Mount only - the refreshers themselves are stable (useCallback without deps).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const skipSnapshot = useCallback((id: string) => {
    setSkippedSnapshotIds((prev) => new Set(prev).add(id));
  }, []);

  const skipSnapshots = useCallback((ids: readonly string[]) => {
    setSkippedSnapshotIds((prev) => {
      const missing = ids.filter(id => !prev.has(id));
      return missing.length ? new Set([...prev, ...missing]) : prev;
    });
  }, []);

  const selectModel = useCallback(
    (id: string) => {
      setSelectedId(id);
      ensureFullModel(id);
      // A trashed entry has no live row to stamp; asking would only produce a 404.
      if (!canEditMetadata() || trashModelsRef.current.some((m) => m.id === id)) return;
      const now = new Date().toISOString();
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, lastViewedAt: now } : m)));
      const key = `lastViewedAt:${id}`;
      beginMutation(key);
      filesApi
        .markFileViewed(id)
        .catch((e) => {
          // A refused request (4xx, e.g. the entry vanished meanwhile) is a silent background miss, not a banner-worthy failure.
          if ((e as { expected?: unknown } | null)?.expected === true) return;
          console.error('[last-viewed] update failed:', e); mutationFailed(key, e);
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
        // A finished import job also reloads the file list. A merge that arrives afterwards
        // (setup adopts a folder through both paths) must not list the same models twice.
        setModels((prev) => {
          const known = new Set(prev.map((m) => m.id));
          const fresh = result.imported.filter((m) => !known.has(m.id));
          return fresh.length ? [...prev, ...fresh] : prev;
        });
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
  // reloads authoritatively instead of filtering locally. Resolves with the
  // reloaded list so callers can tell which files are still there.
  const refetchAfterPartialDelete = useCallback(
    (affectedIds: string[]) => {
      const refreshed = loadFiles();
      setSelectedId((prev) => (prev && affectedIds.includes(prev) ? null : prev));
      refreshFolders();
      refreshTags();
      refreshTrash();
      return refreshed;
    },
    [loadFiles, refreshFolders, refreshTags, refreshTrash],
  );

  const restoreModel = useCallback(
    (id: string) =>
      !canManageFiles() ? Promise.resolve() : isReadOnly() ? fileOp('restore_file', { fileId: id }).then(() => refetchAfterPartialDelete([id])).then(() => {}) : filesApi.restoreFile(id).then(() => {
        setTrashModels((prev) => prev.filter((m) => m.id !== id));
        setSelectedId((prev) => (prev === id ? null : prev));
        refreshFiles();
        refreshFolders();
        refreshTags();
      }),
    [refreshFiles, refreshFolders, refreshTags, refetchAfterPartialDelete],
  );

  const deleteModelPermanently = useCallback(
    (id: string, password?: string) =>
      !canManageFiles() ? Promise.resolve() : isReadOnly() ? (password === undefined ? Promise.resolve() : fileOp('delete_file_permanently', { fileId: id, confirm: true, password }).then(() => refetchAfterPartialDelete([id])).then(() => {})) : filesApi.deleteFilePermanently(id).then(() => {
        setTrashModels((prev) => prev.filter((m) => m.id !== id));
        setSelectedId((prev) => (prev === id ? null : prev));
      }),
    [refetchAfterPartialDelete],
  );

  const emptyTrashAction = useCallback((summary?: TrashSummary, password?: string, onProgress = (_done: number, _total: number) => {}) => {
    if (!canManageFiles()) return Promise.resolve();
    if (isReadOnly()) return summary && password !== undefined ? emptyWebTrash(summary, password, onProgress) : Promise.resolve();
    return filesApi.emptyTrash().then(() => setTrashModels([]));
  }, []);

  const addTag = useCallback(
    (id: string, rawTag: string) => {
      if (!canEditMetadata()) return Promise.resolve();
      // Map translated names of automatic tags (e.g. "Multipart") to the
      // identifier before updating optimistically - otherwise a second tag
      // flashes briefly until the backend has normalized it.
      const tag = canonicalTag(rawTag);
      const current = modelsRef.current.find((m) => m.id === id);
      if (!current || current.tags.includes(tag)) return;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: [...m.tags, tag] } : m)));
      const key = `tags:${id}`;
      beginMutation(key);
      filesApi
        .addTag(id, tag)
        .then(refreshTags)
        .catch((e) => {
          console.error('[tags] adding failed:', e); mutationFailed(key, e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [refreshTags],
  );

  // Deliberately NOT optimistic: the UI should be able to show a name
  // collision directly. The error goes to the caller as a rejection.
  const renameFile = useCallback((id: string, name: string) => {
    if (!canManageFiles()) return Promise.resolve();
    if (isReadOnly()) return fileOp('rename_file', { fileId: id, name: webFileNameParts(name).base }).then(() => refreshFiles());
    return filesApi.renameFile(id, name).then(() => {
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, name } : m)));
      // The new name can change the sort position. Scrolling happens in a
      // useEffect in App.tsx, which reliably runs after the commit
      // (requestAnimationFrame here wouldn't have that guarantee).
      setPendingScrollToId(id);
    });
  }, [refreshFiles]);

  const removeTag = useCallback(
    (id: string, tag: string) => {
      if (!canEditMetadata()) return Promise.resolve();
      const current = models.find((m) => m.id === id);
      if (!current) return;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: m.tags.filter((t) => t !== tag) } : m)));
      const key = `tags:${id}`;
      beginMutation(key);
      filesApi
        .removeTag(id, tag)
        .then(refreshTags)
        .catch((e) => {
          console.error('[tags] removing failed:', e); mutationFailed(key, e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [models, refreshTags],
  );

  const deleteModel = useCallback(
    (id: string) => !canManageFiles() ? Promise.resolve() : isReadOnly() ? fileOpsBatch('delete_files', [id]).then(() => refetchAfterPartialDelete([id])) : filesApi.deleteFile(id).then(() => applyLocalDeletion([id])),
    [applyLocalDeletion, refetchAfterPartialDelete],
  );

  const togglePrintStatus = useCallback(
    (id: string) => {
      if (!canEditMetadata()) return Promise.resolve();
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
          console.error('[print-status] update failed:', e); mutationFailed(key, e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [models],
  );

  const toggleFavorite = useCallback(
    (id: string) => {
      if (!canEditMetadata()) return Promise.resolve();
      const current = models.find((m) => m.id === id);
      if (!current) return;
      const next = !current.favorite;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, favorite: next } : m)));
      const key = `favorite:${id}`;
      beginMutation(key);
      filesApi
        .setFavorite(id, next)
        .catch((e) => {
          console.error('[favorite] update failed:', e); mutationFailed(key, e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [models],
  );

  const addToQueue = useCallback((id: string) => {
    if (!canEditMetadata()) return Promise.resolve();
    const key = `queue:${id}`;
    if (isReadOnly()) beginMutation(key);
    filesApi
      .addToQueue(id)
      .then((position) => {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: position } : m)));
      })
      .catch((e) => { console.error('[queue] adding failed:', e); mutationFailed(key, e); })
      .finally(() => { if (isReadOnly()) void endMutationAndResyncIfSettled(key, id); });
  }, []);

  const removeFromQueue = useCallback((id: string) => {
    if (!canEditMetadata()) return Promise.resolve();
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: null } : m)));
    const key = `queue:${id}`;
    beginMutation(key);
    filesApi
      .removeFromQueue(id)
      .catch((e) => {
        console.error('[queue] removing failed:', e); mutationFailed(key, e);
      })
      .finally(() => {
        void endMutationAndResyncIfSettled(key, id);
      });
  }, []);

  const reorderQueue = useCallback(
    (orderedIds: string[]) => {
      if (!canEditMetadata()) return Promise.resolve();
      const updates: { fileId: string; position: number }[] = [];
      orderedIds.forEach((id, index) => {
        const position = index + 1;
        const current = models.find((m) => m.id === id);
        if (current && current.queuePosition !== position) {
          updates.push({ fileId: id, position });
        }
      });
      if (updates.length === 0) return;
      if (isReadOnly()) updates.forEach(update => beginMutation(`queue:${update.fileId}`));
      setModels((prev) =>
        prev.map((m) => {
          const index = orderedIds.indexOf(m.id);
          return index === -1 ? m : { ...m, queuePosition: index + 1 };
        }),
      );
      // A reorder affects several models; on failure a full refreshFiles()
      // is enough instead of the field+ID counters.
      filesApi.reorderQueue(updates).catch((e) => {
        console.error('[queue] reordering failed:', e); reportMetadataError(e);
        if (isReadOnly()) updates.forEach(update => mutationFailed(`queue:${update.fileId}`, e));
        else void refreshFiles();
      }).finally(() => {
        if (isReadOnly()) updates.forEach(update => { void endMutationAndResyncIfSettled(`queue:${update.fileId}`, update.fileId); });
      });
    },
    [models, refreshFiles],
  );

  const uploadCustomImage = useCallback((id: string, file?: File) => {
    if (isReadOnly()) {
      if (!file || !canEditMetadata()) return Promise.resolve();
      // The answer is empty; the picture itself is reloaded through the image listing.
      return readImageFile(file)
        .then((base64) => filesApi.setCustomImage(id, base64))
        .then(() => invalidateModelImages(id))
        .catch((e) => {
          // The server did the work but cannot repeat its (large) answer: just reload the picture.
          if (!isLostAnswer(e)) throw e;
          invalidateModelImages(id);
        });
    }
    return filesApi
      .uploadCustomImage(id)
      .then((customImage) => {
        if (customImage === null) return;
        invalidateModelImages(id);
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, customImage } : m)));
      });
  }, []);

  const latestSnapshotSaves = useRef(new Map<string, { failed: boolean }>()).current;

  const captureRenderSnapshot = useCallback((id: string, base64: string) => {
    if (!canSaveSnapshots()) return Promise.resolve();
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, hasRenderSnapshot: true } : m)));
    const key = `renderSnapshot:${id}`;
    const attempt = { failed: false };
    latestSnapshotSaves.set(id, attempt);
    beginMutation(key);
    const save = filesApi
      .setRenderSnapshot(id, base64)
      .then(() => {
        // Detail reads started during the write may still contain the old flag.
        mutationEpochs[key]++;
        invalidateModelImages(id);
      })
      .catch((e) => {
        console.error('[render-snapshot] saving failed:', e); reportMetadataError(e);
        attempt.failed = true;
      })
      .finally(() => {
        if (pendingMutationCounts[key] > 1) {
          pendingMutationCounts[key]--;
          return;
        }
        const latestFailed = latestSnapshotSaves.get(id)?.failed;
        latestSnapshotSaves.delete(id);
        if (latestFailed) {
          // Only the latest requested save determines whether reconciliation is needed.
          // A failed write must not immediately enqueue the same failing save.
          skipSnapshot(id);
          // Leave the model retryable on the next session even if resync fails.
          setModels(prev => prev.map(m => m.id === id ? { ...m, hasRenderSnapshot: false } : m));
          void endMutationAndResyncIfSettled(key, id);
        } else {
          delete pendingMutationCounts[key];
        }
      });
    if (import.meta.env.VITE_TARGET === 'web') return save;
  }, []);

  const setModelLicenses = useCallback(async (ids: string[], license: string | null) => {
    if (!canEditMetadata()) return Promise.resolve();
    if (isReadOnly() && ids.length > 1) {
      return metadataBatch([...new Set(ids)], id => filesApi.setLicense([id], license), async () => {
        const summaries = await filesApi.listFileSummaries();
        const selected = new Set(ids);
        const licenses = new Map(summaries.filter(model => selected.has(model.id)).map(model => [model.id, model.license]));
        setModels(previous => previous.map(model => licenses.has(model.id) ? { ...model, license: licenses.get(model.id)! } : model));
      });
    }
    const selected = new Set(ids);
    const startedEpochs = new Map<string, number>();
    let overlapped = false;
    selected.forEach(id => {
      const key = `license:${id}`;
      beginMutation(key);
      startedEpochs.set(id, mutationEpochs[key]);
      if (pendingMutationCounts[key] > 1) overlapped = true;
    });
    try {
      await filesApi.setLicense([...selected], license);
      setModels(prev => prev.map(model => selected.has(model.id) ? { ...model, license } : model));
    } catch (error) {
      reportMetadataError(error);
      throw error;
    } finally {
      const settled = new Map<string, number>();
      selected.forEach(id => {
        if (mutationEpochs[`license:${id}`] !== startedEpochs.get(id)) overlapped = true;
      });
      selected.forEach(id => {
        const key = `license:${id}`;
        if (--pendingMutationCounts[key] === 0) {
          delete pendingMutationCounts[key];
          settled.set(id, mutationEpochs[key]);
        }
      });
      // One slim read reconciles overlapping writes without fetching every selected model's images.
      if (overlapped && settled.size) {
        try {
          const summaries = await filesApi.listFileSummaries();
          const licenses = new Map(summaries.filter(model => settled.has(model.id)
            && pendingMutationCounts[`license:${model.id}`] === undefined
            && mutationEpochs[`license:${model.id}`] === settled.get(model.id))
            .map(model => [model.id, model.license]));
          setModels(prev => prev.map(model => licenses.has(model.id) ? { ...model, license: licenses.get(model.id)! } : model));
        } catch (error) {
          console.error('[license] reloading failed:', error); reportMetadataError(error);
        }
      }
    }
  }, []);

  const setModelSourceUrl = useCallback((id: string, url: string | null) => {
    if (!canEditMetadata()) return Promise.resolve();
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, sourceUrl: url } : m)));
    const key = `sourceUrl:${id}`;
    beginMutation(key);
    filesApi
      .setSourceUrl(id, url)
      .catch((e) => {
        console.error('[source-url] saving failed:', e); mutationFailed(key, e);
      })
      .finally(() => {
        void endMutationAndResyncIfSettled(key, id);
      });
  }, []);

  const rescanMetadata = useCallback((id: string) => {
    if (!canEditMetadata()) return Promise.resolve();
    setRescanFeedback(null);
    const apply = (updated: ModelFile) => {
      invalidateModelImages(id);
      setModels((prev) => prev.map((m) => (m.id === id ? updated : m)));
      // The rescan answer and a plain reload both carry the full ModelFile record.
      setFullyLoadedIds((prev) => new Set(prev).add(id));
      setRescanFeedback({ fileId: id, status: 'success' });
    };
    filesApi
      .rescanFileMetadata(id)
      // A repeated request whose stored answer was too large: the scan itself succeeded, so
      // reading the model again gives the same result as the lost answer.
      .catch((e) => (isLostAnswer(e) ? filesApi.listFilesByIds([id]).then(([fresh]) => fresh ?? Promise.reject(e)) : Promise.reject(e)))
      .then(apply)
      .catch((e) => {
        console.error('[rescan] rescan failed:', e); reportMetadataError(e);
        const err = toAppError(e);
        setRescanFeedback({ fileId: id, status: 'error', message: err.message, unexpected: err.unexpected });
      });
  }, []);

  return {
    initialLoading, initialLoadFailed, models, setModels,
    folders, tags, trashModels,
    selectedId, setSelectedId,
    skippedSnapshotIds, skipSnapshot, skipSnapshots, pendingSnapshotIds,
    rescanFeedback,
    pendingScrollToId, setPendingScrollToId,
    refreshFolders, refreshFiles, refreshTags, refreshTrash,
    selectModel, mergeImported, ensureFullModel,
    restoreModel, deleteModelPermanently, emptyTrashAction,
    addTag, removeTag, renameFile, deleteModel,
    togglePrintStatus, toggleFavorite,
    addToQueue, removeFromQueue, reorderQueue,
    uploadCustomImage, captureRenderSnapshot, setModelSourceUrl, setModelLicenses,
    rescanMetadata,
    applyLocalDeletion, refetchAfterPartialDelete,
  };
}
