import { useCallback, useEffect, useMemo, useState } from 'react';
import * as filesApi from '../lib/api/files';
import * as foldersApi from '../lib/api/folders';
import * as catalogMetaApi from '../lib/api/catalogMeta';
import type { ModelFile, ModelFileSummary, Folder, TagCount, CreatorCount, SavedFilter, ImportResultDto } from '../types';

// Bettet eine schlanke `ModelFileSummary` (siehe Finding M-01) in die volle
// `ModelFile`-Form ein, damit die Grid-/Listen-Ansicht sowie alle
// bestehenden Komponenten/Hooks weiterhin denselben `ModelFile`-Typ sehen,
// ohne dass jede Stelle im Baum angepasst werden muss. Die hier NICHT von
// list_file_summaries gelieferten Felder (materials/tags/renderSnapshotImage/
// customImage/sliceInfo/costEstimate/creator/sourceUrl/lastViewedAt) werden
// mit neutralen Defaults gefuellt und erst nachtraeglich per
// `ensureFullModel()`/`listFilesByIds([id])` echt befuellt, sobald ein
// Modell ausgewaehlt oder die Detailseite geoeffnet wird.
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
    lastViewedAt: null,
    creator: null,
    customImage: null,
    thumbnailImage: s.thumbnailImage,
    renderSnapshotImage: null,
    sourceUrl: null,
    queuePosition: s.queuePosition,
    favorite: s.favorite,
    deletedAt: null,
  };
}

export function useCatalogStore() {
  const [models, setModels] = useState<ModelFile[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [creators, setCreators] = useState<CreatorCount[]>([]);
  const [savedFilters, setSavedFilters] = useState<SavedFilter[]>([]);
  const [trashModels, setTrashModels] = useState<ModelFile[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // IDs, fuer die bereits die volle ModelFile-Auskunft (Materials/Tags/
  // Bilder/Metadata) per listFilesByIds nachgeladen wurde - verhindert
  // wiederholtes Nachladen, solange kein refreshFiles() die Liste wieder auf
  // schlanke Summaries zuruecksetzt (siehe ensureFullModel).
  const [fullyLoadedIds, setFullyLoadedIds] = useState<Set<string>>(new Set());
  const [skippedSnapshotIds, setSkippedSnapshotIds] = useState<Set<string>>(new Set());
  // Pro Modell-ID statt global, damit ein Fehler/Erfolg von Modell A nicht
  // unter Modell B stehen bleibt, wenn der Nutzer zwischendurch die
  // Detailseite wechselt - kein separater Reset-Effekt nötig, da der Zugriff
  // in der UI immer gegen detailModel.id abgeglichen wird.
  const [rescanFeedback, setRescanFeedback] = useState<
    { fileId: string; status: 'success' | 'error'; message?: string } | null
  >(null);

  const pendingSnapshotIds = useMemo(
    () => models.filter((m) => m.renderSnapshotImage === null && !skippedSnapshotIds.has(m.id)).map((m) => m.id),
    [models, skippedSnapshotIds],
  );

  const refreshFolders = useCallback(() => foldersApi.listFolders().then(setFolders), []);
  // Katalog-Uebersicht laedt seit Finding M-01 nur noch die schlanke
  // Summary-Projektion statt der vollen ModelFile-Liste (Materials/Tags/
  // Metadata sowie render_snapshot_png/custom_image_png wurden bisher pro
  // Zeile mitgeladen, obwohl die Grid-/Listen-Ansicht sie gar nicht
  // anzeigt). Ein refreshFiles() setzt damit alle Eintraege wieder auf den
  // schlanken Stand zurueck - bereits "hochgestufte" (fullyLoadedIds)
  // Eintraege werden beim naechsten ensureFullModel()-Aufruf einfach erneut
  // nachgeladen, das ist unkritisch (kein Datenverlust, nur ein zusaetzlicher
  // Roundtrip).
  const refreshFiles = useCallback(() => {
    setFullyLoadedIds(new Set());
    return filesApi.listFileSummaries().then((summaries) => setModels(summaries.map(summaryToModelFile)));
  }, []);
  const refreshTags = useCallback(() => catalogMetaApi.listTagCounts().then(setTags), []);
  const refreshCreators = useCallback(() => catalogMetaApi.listCreators().then(setCreators), []);
  const refreshSavedFilters = useCallback(() => catalogMetaApi.listSavedFilters().then(setSavedFilters), []);
  const refreshTrash = useCallback(() => filesApi.listTrash().then(setTrashModels), []);

  // Laedt die vollen Modelldaten (Materials/Tags/Bilder/Metadata) fuer genau
  // ein Modell nach, sobald es tatsaechlich gebraucht wird (Auswahl in der
  // Liste oder Oeffnen der Detailseite) - siehe Task-6-Brief Step 4b/6.
  // Kein Einzeldatensatz-Command: `listFilesByIds` wird mit einer Liste der
  // Laenge 1 aufgerufen. Ersetzt den bisherigen Eintrag in `models` in-place,
  // damit `models.find(id)` (selected/detailModel in App.tsx) danach die
  // vollen Daten liefert, ohne dass Grid/Liste selbst umgebaut werden
  // muessen.
  const ensureFullModel = useCallback(
    (id: string) => {
      if (fullyLoadedIds.has(id) || !models.some((m) => m.id === id)) return;
      filesApi
        .listFilesByIds([id])
        .then((full) => {
          if (full.length === 0) return;
          setModels((prev) =>
            prev.map((m) => (m.id === id ? { ...full[0], lastViewedAt: m.lastViewedAt ?? full[0].lastViewedAt } : m)),
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
    filesApi.listFileSummaries().then((summaries) => {
      const mapped = summaries.map(summaryToModelFile);
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
      filesApi.markFileViewed(id).catch((e) => {
        console.error('[last-viewed] Aktualisieren fehlgeschlagen:', e);
      });
    },
    [ensureFullModel],
  );

  const mergeImported = useCallback(
    (result: ImportResultDto) => {
      if (result.imported.length) {
        setModels((prev) => [...prev, ...result.imported]);
        // Frisch importierte Dateien kommen bereits als volle ModelFile-
        // Datensaetze vom Import-Command - kein erneutes Nachladen noetig.
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

  // Fuer Aufrufer, bei denen delete_files einzelne IDs stillschweigend
  // uebersprungen haben kann (siehe deleteSelectedCleanupFiles) - laedt
  // die Modell-Liste autoritativ neu statt lokal zu filtern.
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
    refreshFolders, refreshFiles, refreshTags, refreshCreators, refreshTrash,
    selectModel, mergeImported, ensureFullModel,
    restoreModel, deleteModelPermanently, emptyTrashAction,
    addTag, removeTag, deleteModel,
    togglePrintStatus, toggleFavorite,
    addToQueue, removeFromQueue, reorderQueue,
    uploadCustomImage, captureRenderSnapshot, setModelSourceUrl,
    rescanMetadata,
    applyLocalDeletion, refetchAfterPartialDelete,
  };
}
