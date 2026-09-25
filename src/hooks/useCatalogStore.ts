import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import * as filesApi from '../lib/api/files';
import * as foldersApi from '../lib/api/folders';
import * as catalogMetaApi from '../lib/api/catalogMeta';
import { canonicalTag } from '../lib/autoTags';
import type { ModelFile, ModelFileSummary, Folder, TagCount, CreatorCount, SavedFilter, ImportResultDto } from '../types';

// Bettet eine schlanke `ModelFileSummary` in die volle `ModelFile`-Form ein,
// damit alle Komponenten denselben Typ sehen. Nicht gelieferte Felder
// (materials, customImage, sliceInfo, costEstimate, sourceUrl) bekommen
// neutrale Defaults und werden erst per `ensureFullModel()` nachgeladen.
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

// Praefixe im "<feld>:<id>"-Schema der Mutationszaehler, gemappt auf den
// ModelFile-Feldnamen. ensureFullModel() behaelt fuer diese Felder den
// aktuellen Wert, wenn waehrend seines Fetches eine Mutation lief.
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
  const [creators, setCreators] = useState<CreatorCount[]>([]);
  const [savedFilters, setSavedFilters] = useState<SavedFilter[]>([]);
  const [trashModels, setTrashModels] = useState<ModelFile[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // IDs mit bereits nachgeladenen vollen Daten; refreshFiles() setzt die Liste
  // wieder auf Summaries zurueck.
  const [fullyLoadedIds, setFullyLoadedIds] = useState<Set<string>>(new Set());
  const [skippedSnapshotIds, setSkippedSnapshotIds] = useState<Set<string>>(new Set());
  // IDs, fuer die laut Summary schon ein Snapshot in der DB liegt; sie brauchen
  // keinen neuen.
  const [summaryConfirmedSnapshotIds, setSummaryConfirmedSnapshotIds] = useState<Set<string>>(new Set());
  // Pro Modell-ID, damit Feedback von Modell A nicht unter Modell B stehen
  // bleibt, wenn der Nutzer die Detailseite wechselt.
  const [rescanFeedback, setRescanFeedback] = useState<
    { fileId: string; status: 'success' | 'error'; message?: string } | null
  >(null);
  // Einmaliger Scroll-Wunsch nach dem naechsten Commit (siehe renameFile);
  // App.tsx setzt ihn danach wieder auf null.
  const [pendingScrollToId, setPendingScrollToId] = useState<string | null>(null);

  // Rollback fuer fehlgeschlagene optimistische Mutationen: pro "<feld>:<id>"
  // wird die Zahl der laufenden Backend-Aufrufe gezaehlt. Faellt sie auf 0,
  // wird der Datensatz einmal per listFilesByIds([id]) neu geladen; nach
  // Abschluss aller Aufrufe stimmt das Backend immer, egal welcher Aufruf
  // gescheitert ist. (Ein "auf previous zuruecksetzen" oder "nur der neueste
  // darf zurueckrollen" ist bei ueberlappenden Fehlschlaegen falsch.)
  //
  // Die Epoch pro Feld+ID verhindert, dass ein verspaeteter, veralteter Resync
  // einen neueren ueberschreibt: uebernommen wird nur, wenn Zaehler und Epoch
  // seit Start des Resyncs unveraendert sind.
  //
  // Refs statt State: die Zaehler selbst sollen keine Re-Renders ausloesen.
  const pendingMutationCounts = useRef<Record<string, number>>({}).current;
  const mutationEpochs = useRef<Record<string, number>>({}).current;

  function beginMutation(key: string): void {
    pendingMutationCounts[key] = (pendingMutationCounts[key] ?? 0) + 1;
    mutationEpochs[key] = (mutationEpochs[key] ?? 0) + 1;
  }

  // Immer im .finally() der Mutation aufrufen, egal ob Erfolg oder Fehler.
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
      // Nur uebernehmen, wenn waehrend des await keine neue Mutation startete und
      // kein neuerer Resync schon einen aktuelleren Stand uebernommen hat.
      if (fresh && pendingMutationCounts[key] === undefined && mutationEpochs[key] === epochAtResyncStart) {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, ...fresh } : m)));
      }
    } catch (e) {
      console.error(`[resync] Nachladen von ${key} fehlgeschlagen:`, e);
      // Kein weiterer Rollback; der naechste refreshFiles() gleicht ab.
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

  // Summaries und Datei-Tag-Zuordnung parallel in je einer Abfrage laden und
  // die Tags client-seitig einmischen (fuer die Tag-Filterung).
  const loadSummariesWithTags = useCallback(() => {
    return Promise.all([filesApi.listFileSummaries(), filesApi.listAllFileTags()]).then(([summaries, tagsByFileId]) => {
      setSummaryConfirmedSnapshotIds(new Set(summaries.filter((s) => s.hasRenderSnapshot).map((s) => s.id)));
      return summaries.map((s) => ({ ...summaryToModelFile(s), tags: tagsByFileId[s.id] ?? [] }));
    });
  }, []);

  // Laedt nur die schlanken Summaries. Schon hochgestufte Eintraege werden beim
  // naechsten ensureFullModel() einfach erneut nachgeladen.
  const refreshFiles = useCallback(() => {
    setFullyLoadedIds(new Set());
    return loadSummariesWithTags().then(setModels);
  }, [loadSummariesWithTags]);
  const refreshTags = useCallback(() => catalogMetaApi.listTagCounts().then(setTags), []);
  const refreshCreators = useCallback(() => catalogMetaApi.listCreators().then(setCreators), []);
  const refreshSavedFilters = useCallback(() => catalogMetaApi.listSavedFilters().then(setSavedFilters), []);
  const refreshTrash = useCallback(() => filesApi.listTrash().then(setTrashModels), []);

  // Laedt die vollen Daten eines Modells nach, sobald es gebraucht wird
  // (Auswahl oder Detailseite), und ersetzt den Eintrag in `models`.
  const ensureFullModel = useCallback(
    (id: string) => {
      if (fullyLoadedIds.has(id) || !models.some((m) => m.id === id)) return;
      // Epoch-Schnappschuss vor dem Fetch: laeuft parallel eine Mutation samt
      // Resync, darf das dann veraltete Fetch-Ergebnis deren Wert nicht
      // ueberschreiben.
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
                  // Fuer dieses Feld lief waehrend des Fetches eine Mutation: aktuellen
                  // Wert behalten, alle anderen Felder aus `full[0]` nehmen.
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
    refreshCreators();
    refreshSavedFilters();
    refreshTrash();
    // Nur beim Mount - die Refresher selbst sind stabil (useCallback ohne Deps).
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
        // Frisch importierte Dateien kommen schon mit vollen Daten.
        setFullyLoadedIds((prev) => {
          const next = new Set(prev);
          result.imported.forEach((m) => next.add(m.id));
          return next;
        });
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

  // Fuer Aufrufer, bei denen delete_files IDs still uebersprungen haben kann:
  // laedt autoritativ neu statt lokal zu filtern.
  const refetchAfterPartialDelete = useCallback(
    (affectedIds: string[]) => {
      refreshFiles();
      setSelectedId((prev) => (prev && affectedIds.includes(prev) ? null : prev));
      refreshFolders();
      refreshTags();
      refreshCreators();
      refreshTrash();
    },
    [refreshFiles, refreshFolders, refreshTags, refreshCreators, refreshTrash],
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
    (id: string, rawTag: string) => {
      // Uebersetzte Namen automatischer Tags (z. B. "Multipart") auf die
      // Kennung abbilden, bevor optimistisch aktualisiert wird - sonst
      // blitzt kurz ein zweiter Tag auf, bis das Backend normalisiert hat.
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

  // Bewusst NICHT optimistisch: eine Namenskollision soll die UI direkt
  // anzeigen koennen. Der Fehler geht als Rejection an den Aufrufer.
  const renameFile = useCallback((id: string, name: string) => {
    return filesApi.renameFile(id, name).then(() => {
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, name } : m)));
      // Der neue Name kann die Sortierposition aendern. Gescrollt wird in einem
      // useEffect in App.tsx, der sicher nach dem Commit laeuft
      // (requestAnimationFrame hier haette diese Garantie nicht).
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
      // Ein Reorder betrifft mehrere Modelle; bei einem Fehler reicht ein voller
      // refreshFiles() statt der Feld+ID-Zaehler.
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
        // rescan_file_metadata liefert bereits den vollen ModelFile-Datensatz.
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
    folders, tags, creators, savedFilters, trashModels,
    selectedId, setSelectedId,
    skippedSnapshotIds, skipSnapshot, pendingSnapshotIds,
    rescanFeedback,
    pendingScrollToId, setPendingScrollToId,
    refreshFolders, refreshFiles, refreshTags, refreshCreators, refreshTrash,
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
