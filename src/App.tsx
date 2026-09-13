import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Header } from './components/Header';
import { Rail } from './components/Rail';
import { Sidebar } from './components/Sidebar';
import { ModelGrid } from './components/ModelGrid';
import { ModelList } from './components/ModelList';
import { DetailPanel } from './components/DetailPanel';
import { ModelDetailPage } from './components/ModelDetailPage';
import { ContextMenu } from './components/ContextMenu';
import { FilamentView } from './components/FilamentView';
import { ImportSummaryBanner } from './components/ImportSummaryBanner';
import { CatalogCleanupDialog } from './components/CatalogCleanupDialog';
import { CollectionsGallery } from './components/CollectionsGallery';
import { BackgroundSnapshotRenderer } from './components/BackgroundSnapshotRenderer';
import { useTheme } from './hooks/useTheme';
import { useUiDensity } from './hooks/UiDensityContext';
import { useSlicers } from './hooks/useSlicers';
import { useDisplayPreference } from './hooks/useDisplayPreference';
import { useT } from './i18n/LanguageContext';
import { isFileInFolderOrDescendant, isFolderSelfOrDescendant } from './lib/folderTree';
import { MoveToast } from './components/MoveToast';
import { CatalogSetupDialog } from './components/CatalogSetupDialog';
import { useCatalogBaseDir } from './hooks/useCatalogBaseDir';
import type { ModelFile, Folder, TagCount, CreatorCount, ViewMode, SortKey, SavedFilter, CatalogIssues, Collection, ImportResultDto } from './types';

export default function App() {
  const { setting, setTheme } = useTheme();
  const t = useT();
  const { density, setDensity } = useUiDensity();
  const { slicers, lastUsedId, addSlicer, removeSlicer, setLastUsed, mergeDetected } = useSlicers();
  const { preference: displayPreference, setPreference: setDisplayPreference } = useDisplayPreference();
  const { catalogBaseDir, setCatalogBaseDir, setupSeen, markSetupSeen } = useCatalogBaseDir();
  const [setupDialogOpen, setSetupDialogOpen] = useState(!setupSeen);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [slicerError, setSlicerError] = useState<string | null>(null);
  const [view, setView] = useState<ViewMode>('grid');
  const [sort, setSort] = useState<SortKey>('name');
  const [query, setQuery] = useState('');
  const [activeFolderId, setActiveFolderId] = useState('all');
  const [activeTag, setActiveTag] = useState<string | null>(null);
  const [, setCreators] = useState<CreatorCount[]>([]);
  const [activeCreator] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedForBulk, setSelectedForBulk] = useState<Set<string>>(new Set());
  const [confirmBulkDelete, setConfirmBulkDelete] = useState(false);
  const [addToCollectionMenuOpen, setAddToCollectionMenuOpen] = useState(false);
  const [detailModelId, setDetailModelId] = useState<string | null>(null);
  const [models, setModels] = useState<ModelFile[]>([]);
  const [skippedSnapshotIds, setSkippedSnapshotIds] = useState<Set<string>>(new Set());
  const pendingSnapshotIds = useMemo(
    () => models.filter((m) => m.renderSnapshotImage === null && !skippedSnapshotIds.has(m.id)).map((m) => m.id),
    [models, skippedSnapshotIds],
  );
  const [folders, setFolders] = useState<Folder[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);
  const [mainView, setMainView] = useState<'catalog' | 'filament' | 'trash'>('catalog');
  const [trashModels, setTrashModels] = useState<ModelFile[]>([]);
  const [confirmEmptyTrash, setConfirmEmptyTrash] = useState(false);
  const [importBanner, setImportBanner] = useState<{ imported: number; duplicates: number } | null>(null);
  const [, setSavedFilters] = useState<SavedFilter[]>([]);
  const [cleanupDialogOpen, setCleanupDialogOpen] = useState(false);
  const [cleanupIssues, setCleanupIssues] = useState<CatalogIssues | null>(null);
  const [cleanupScanning, setCleanupScanning] = useState(false);
  const [cleanupError, setCleanupError] = useState<string | null>(null);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [activeCollection, setActiveCollection] = useState<string | null>(null);
  const [collectionsGalleryOpen, setCollectionsGalleryOpen] = useState(false);
  const [collectionModels, setCollectionModels] = useState<ModelFile[]>([]);

  // Maus-basiertes Drag-Tracking fuer physisches Verschieben von Dateien/
  // Ordnern (Task 7) - folgt demselben Muster wie der Warteschlangen-Reorder
  // in Sidebar.tsx und der Karten-Reorder in ModelGrid.tsx: kein natives
  // HTML5-DnD (draggable/onDragStart/onDragOver/onDrop), da Tauri/WebKitGTK
  // das nicht zuverlaessig unterstuetzt (dragDropEnabled faengt native
  // Drag-Sessions auf Fensterebene ab, siehe Kommentare dort).
  const [draggedFileId, setDraggedFileId] = useState<string | null>(null);
  const [draggedFolderId, setDraggedFolderId] = useState<string | null>(null);
  const [dragOverFolderId, setDragOverFolderId] = useState<string | null>(null);
  const [moveToast, setMoveToast] = useState<{ from: string; to: string; error?: boolean } | null>(null);

  const refreshCollectionModels = (collectionId: string) =>
    invoke<ModelFile[]>('list_collection_files', { collectionId }).then(setCollectionModels);

  const refreshFolders = () => invoke<Folder[]>('list_folders').then(setFolders);
  const refreshFiles = () => invoke<ModelFile[]>('list_files').then(setModels);
  const refreshTags = () => invoke<TagCount[]>('list_tag_counts').then(setTags);
  const refreshCreators = () => invoke<CreatorCount[]>('list_creators').then(setCreators);
  const refreshSavedFilters = () => invoke<SavedFilter[]>('list_saved_filters').then(setSavedFilters);
  const refreshTrash = () => invoke<ModelFile[]>('list_trash').then(setTrashModels);
  const refreshCollections = () => invoke<Collection[]>('list_collections').then(setCollections);

  const restoreModel = (id: string) => {
    invoke('restore_file', { fileId: id }).then(() => {
      setTrashModels((prev) => prev.filter((m) => m.id !== id));
      setSelectedId((prev) => (prev === id ? null : prev));
      invoke<ModelFile[]>('list_files').then(setModels);
      refreshFolders();
      refreshTags();
      refreshCreators();
    });
  };

  const deleteModelPermanently = (id: string) => {
    invoke('delete_file_permanently', { fileId: id }).then(() => {
      setTrashModels((prev) => prev.filter((m) => m.id !== id));
      setSelectedId((prev) => (prev === id ? null : prev));
    });
  };

  const emptyTrash = () => {
    invoke('empty_trash').then(() => {
      setTrashModels([]);
      setConfirmEmptyTrash(false);
    });
  };

  const mergeImported = (result: ImportResultDto) => {
    if (result.imported.length) {
      setModels((prev) => [...prev, ...result.imported]);
      setSelectedId(result.imported[result.imported.length - 1].id);
      refreshFolders();
      refreshTags();
      refreshCreators();
    }
    if (result.duplicateCount > 0) {
      setImportBanner({ imported: result.imported.length, duplicates: result.duplicateCount });
    }
  };

  const selectModel = (id: string) => {
    setSelectedId(id);
    const now = new Date().toISOString();
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, lastViewedAt: now } : m)));
    invoke('mark_file_viewed', { fileId: id }).catch((e) => {
      console.error('[last-viewed] Aktualisieren fehlgeschlagen:', e);
    });
  };

  useEffect(() => {
    invoke<ModelFile[]>('list_files').then((files) => {
      setModels(files);
      setSelectedId((prev) => prev ?? files[0]?.id ?? null);
    });
    refreshFolders();
    refreshTags();
    refreshCreators();
    refreshSavedFilters();
    refreshTrash();
    refreshCollections();
    invoke<{ name: string; path: string }[]>('scan_installed_slicers')
      .then(mergeDetected)
      .catch((e) => {
        // Rein komfortsteigerndes Feature - ein Fehlschlag (z.B. Command
        // aus irgendeinem Grund nicht verfuegbar) darf die App nicht
        // beeintraechtigen, nur geloggt werden.
        console.warn('[slicer-scan] Automatische Slicer-Erkennung fehlgeschlagen:', e);
      });
  }, []);

  useEffect(() => {
    if (activeCollection) {
      refreshCollectionModels(activeCollection);
    } else {
      setCollectionModels([]);
    }
  }, [activeCollection]);

  useEffect(() => {
    if (!detailModelId) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')) return;
      setDetailModelId(null);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [detailModelId]);

  useEffect(() => {
    setSelectedForBulk(new Set());
    setConfirmBulkDelete(false);
  }, [activeFolderId, activeTag, activeCreator, query, activeCollection, collectionsGalleryOpen]);

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== 'drop') return;
      if (mainView !== 'catalog') return;
      invoke<ImportResultDto>('import_dropped', { paths: event.payload.paths }).then(mergeImported);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [mainView]);

  // Karte in ModelGrid ueberschreitet die Bewegungsschwelle -> echter Drag
  // einer Datei beginnt. Wird von ModelGrid nur im nicht-reorderbaren
  // Katalog-Zweig aufgerufen (siehe dortigen Kommentar).
  const onDragFileStart = (id: string) => setDraggedFileId(id);

  // Baum-Zeile in FolderTree wird per Mousedown als Drag-Quelle markiert
  // (Ordner-auf-Ordner-Verschieben, Step 7). Ein einfacher Klick ohne
  // anschliessendes Hovern ueber eine andere Zeile loest nie einen Move aus,
  // da dragOverFolderId dann null bleibt (siehe Mouseup-Handler unten).
  const onDragFolderStart = (id: string) => setDraggedFolderId(id);

  // Baum-Zeile wird waehrend eines aktiven Drags (Datei oder Ordner)
  // betreten -> Drop-Ziel-Highlight setzen. Beim Ordner-Drag wird die
  // Zyklus-Vorabpruefung (eigener Unterbaum/sich selbst) hier clientseitig
  // dupliziert, damit gar kein Highlight auf einem ungueltigen Ziel
  // erscheint - die serverseitige Pruefung in move_folder (Task 5) bleibt
  // die verbindliche Instanz.
  const handleFolderMouseEnter = (id: string) => {
    if (!draggedFileId && !draggedFolderId) return;
    if (draggedFolderId && isFolderSelfOrDescendant(id, draggedFolderId, folders)) {
      setDragOverFolderId(null);
      return;
    }
    setDragOverFolderId(id);
  };

  const onCreateFolder = (parentId: string | null, name: string) => {
    invoke('create_folder', { parentId, name })
      .then(() => refreshFolders())
      .catch((e) => {
        console.error('[folders] Anlegen fehlgeschlagen:', e);
        // Fehler sichtbar in der Naehe des Ordnerbaums zeigen (MoveToast
        // wiederverwendet mit error:true) statt nur in das Settings-only
        // catalogBackupError zu routen, das ohne geoeffnetes Rail-Panel
        // unsichtbar bleibt.
        setMoveToast({ from: name, to: String(e), error: true });
      });
  };

  // Globaler mouseup-Handler fuer das Verschieben einer Datei per Maus-Drag
  // auf eine Baum-Zeile - exakt dasselbe useEffect-Muster wie
  // Sidebar.tsx:65-84 (Warteschlangen-Reorder).
  useEffect(() => {
    if (!draggedFileId) return;
    const handleMouseUp = () => {
      const fileId = draggedFileId;
      const folderId = dragOverFolderId;
      setDraggedFileId(null);
      setDragOverFolderId(null);
      if (!folderId) return;
      const file = models.find((m) => m.id === fileId);
      const targetFolder = folders.find((f) => f.id === folderId);
      if (!file || !targetFolder) return;
      invoke('move_file_to_folder', { fileId, folderId })
        .then(() => {
          setMoveToast({ from: file.name, to: targetFolder.path });
          refreshFolders();
          refreshFiles();
        })
        .catch((e) => {
          console.error('[folders] Datei verschieben fehlgeschlagen:', e);
          setMoveToast({ from: file.name, to: String(e), error: true });
        });
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [draggedFileId, dragOverFolderId, models, folders]);

  // Analoger mouseup-Handler fuer das Verschieben eines Ordners per
  // Maus-Drag auf eine andere Baum-Zeile (Step 7). Die Zyklus-Pruefung wird
  // hier zusaetzlich wiederholt (nicht nur beim Hover-Highlight), damit ein
  // ungueltiges Ziel unter keinen Umstaenden einen invoke-Aufruf ausloest -
  // move_folder auf der Rust-Seite lehnt es ohnehin verbindlich ab.
  useEffect(() => {
    if (!draggedFolderId) return;
    const handleMouseUp = () => {
      const folderId = draggedFolderId;
      const targetId = dragOverFolderId;
      setDraggedFolderId(null);
      setDragOverFolderId(null);
      if (!targetId || targetId === folderId) return;
      if (isFolderSelfOrDescendant(targetId, folderId, folders)) return;
      const folder = folders.find((f) => f.id === folderId);
      const targetFolder = folders.find((f) => f.id === targetId);
      if (!folder || !targetFolder) return;
      invoke('move_folder', { folderId, newParentId: targetId })
        .then(() => {
          setMoveToast({ from: folder.name, to: targetFolder.path });
          refreshFolders();
          refreshFiles();
        })
        .catch((e) => {
          console.error('[folders] Ordner verschieben fehlgeschlagen:', e);
          setMoveToast({ from: folder.name, to: String(e), error: true });
        });
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [draggedFolderId, dragOverFolderId, folders]);

  const filtered = useMemo(() => {
    return models
      .filter((m) => activeFolderId === 'all' || isFileInFolderOrDescendant(m.folderId, activeFolderId, folders))
      .filter((m) => !activeTag || m.tags.includes(activeTag))
      .filter((m) => !activeCreator || m.creator === activeCreator)
      .filter((m) => !query || m.name.toLowerCase().includes(query.toLowerCase()))
      .sort((a, b) => {
        if (sort === 'name') return a.name.localeCompare(b.name);
        if (sort === 'size') return a.fileSizeBytes - b.fileSizeBytes;
        if (sort === 'date') return b.importedAt.localeCompare(a.importedAt);
        if (sort === 'viewed') return (b.lastViewedAt ?? '').localeCompare(a.lastViewedAt ?? '');
        return 0;
      });
  }, [models, activeFolderId, activeTag, activeCreator, query, sort, folders]);

  const queue = useMemo(
    () =>
      models
        .filter((m) => m.queuePosition !== null)
        .sort((a, b) => (a.queuePosition ?? 0) - (b.queuePosition ?? 0)),
    [models],
  );

  const selected = models.find((m) => m.id === selectedId) ?? null;
  const detailModel = detailModelId ? models.find((m) => m.id === detailModelId) ?? null : null;
  const contextModel = contextMenu ? models.find((m) => m.id === contextMenu.modelId) ?? null : null;

  const setLocalTags = (id: string, next: string[]) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: next } : m)));
  };

  const addTag = (id: string, tag: string) => {
    const current = models.find((m) => m.id === id);
    if (!current || current.tags.includes(tag)) return;
    setLocalTags(id, [...current.tags, tag]);
    invoke('add_tag', { fileId: id, tag }).then(refreshTags);
  };

  const removeTag = (id: string, tag: string) => {
    const current = models.find((m) => m.id === id);
    if (!current) return;
    setLocalTags(id, current.tags.filter((t) => t !== tag));
    invoke('remove_tag', { fileId: id, tag }).then(refreshTags);
  };

  const importFiles = () =>
    invoke<ImportResultDto>('import_files').then((result) => {
      mergeImported(result);
      if (catalogBaseDir) {
        const targetFolder =
          activeFolderId !== 'all'
            ? activeFolderId
            : folders.find((f) => f.path === catalogBaseDir && !f.parentId)?.id;
        if (targetFolder) {
          result.imported.forEach((file) => {
            invoke('move_file_to_folder', { fileId: file.id, folderId: targetFolder }).catch((e) =>
              console.error('[import] Einsortieren fehlgeschlagen:', e),
            );
          });
          refreshFolders();
        }
      }
    });
  const importFolder = () => invoke<ImportResultDto>('import_folder').then(mergeImported);
  const importFolderAsCollection = () =>
    invoke<ImportResultDto>('import_folder_as_collection').then(mergeImported).then(() => refreshCollections());

  const openInSlicer = (id: string, slicerId?: string) => {
    const model = models.find((m) => m.id === id);
    if (!model) return;
    if (slicers.length === 0) {
      setSettingsOpen(true);
      return;
    }
    const target = slicerId
      ? slicers.find((s) => s.id === slicerId)
      : slicers.find((s) => s.id === lastUsedId) ?? slicers[0];
    if (!target) {
      setSettingsOpen(true);
      return;
    }
    setLastUsed(target.id);
    setSlicerError(null);
    invoke('open_in_slicer', { slicerPath: target.path, filePath: model.path }).catch((e) => {
      console.error('[slicer] Start fehlgeschlagen:', e);
      setSlicerError(String(e));
    });
  };

  // Pro Modell-ID statt global, damit ein Fehler/Erfolg von Modell A nicht
  // unter Modell B stehen bleibt, wenn der Nutzer zwischendurch die
  // Detailseite wechselt - kein separater Reset-Effekt nötig, da der Zugriff
  // unten immer gegen detailModel.id abgeglichen wird.
  const [rescanFeedback, setRescanFeedback] = useState<
    { fileId: string; status: 'success' | 'error'; message?: string } | null
  >(null);

  const [catalogBackupError, setCatalogBackupError] = useState<string | null>(null);

  const CATALOG_SETTINGS_KEYS = [
    '3mf-katalog-theme',
    '3mf-katalog-display-preference',
    '3mf-katalog-language',
    '3mf-katalog-slicers',
    '3mf-katalog-density',
  ] as const;

  const exportCatalog = () => {
    setCatalogBackupError(null);
    const settings: Record<string, string | null> = {};
    for (const key of CATALOG_SETTINGS_KEYS) {
      settings[key] = localStorage.getItem(key);
    }
    invoke('export_catalog', { settingsJson: JSON.stringify(settings) }).catch((e) => {
      console.error('[catalog-backup] Export fehlgeschlagen:', e);
      setCatalogBackupError(String(e));
    });
  };

  const importCatalog = () => {
    setCatalogBackupError(null);
    invoke<{ imported: boolean; settingsJson: string | null }>('import_catalog')
      .then((result) => {
        if (!result.imported) return;
        if (result.settingsJson) {
          // Ein Fehler beim Wiederherstellen der Einstellungen darf den
          // erfolgreichen Katalog-Import nicht als Fehlschlag erscheinen
          // lassen (Finding I2) - daher eigenes try/catch statt im
          // aeusseren .catch() der Promise-Kette landen zu lassen.
          try {
            const settings = JSON.parse(result.settingsJson) as Record<string, string | null>;
            for (const key of CATALOG_SETTINGS_KEYS) {
              const value = settings[key];
              if (value === null || value === undefined) {
                localStorage.removeItem(key);
              } else {
                localStorage.setItem(key, value);
              }
            }
          } catch (e) {
            console.error('[catalog-backup] Einstellungen konnten nicht wiederhergestellt werden:', e);
          }
        }
        // Backend hat AppState.db bereits auf den neu importierten Katalog
        // umverbunden - ohne sofortigen Reload wuerde das Frontend weiter
        // veraltete Modell-IDs aus dem alten Katalog anzeigen und Aktionen
        // (Loeschen/Favorit/Tag) koennten versehentlich falsche Datensaetze
        // im neuen Katalog treffen (Finding C1). Ein voller Reload laedt die
        // React-App komplett neu und holt alle Daten gegen die jetzt aktive
        // DB neu ab.
        window.location.reload();
      })
      .catch((e) => {
        console.error('[catalog-backup] Import fehlgeschlagen:', e);
        setCatalogBackupError(String(e));
      });
  };

  const rescanMetadata = (id: string) => {
    setRescanFeedback(null);
    invoke<ModelFile>('rescan_file_metadata', { fileId: id })
      .then((updated) => {
        setModels((prev) => prev.map((m) => (m.id === id ? updated : m)));
        setRescanFeedback({ fileId: id, status: 'success' });
      })
      .catch((e) => {
        console.error('[rescan] Neu-Einlesen fehlgeschlagen:', e);
        setRescanFeedback({ fileId: id, status: 'error', message: String(e) });
      });
  };

  const deleteModel = (id: string) => {
    invoke('delete_file', { fileId: id }).then(() => {
      setModels((prev) => prev.filter((m) => m.id !== id));
      setSelectedId((prev) => (prev === id ? null : prev));
      refreshFolders();
      refreshTags();
      refreshCreators();
      refreshTrash();
    });
  };

  const togglePrintStatus = (id: string) => {
    const current = models.find((m) => m.id === id);
    if (!current) return;
    const next = current.printStatus === 'printed' ? 'not_printed' : 'printed';
    setModels((prev) =>
      prev.map((m) =>
        m.id === id
          ? { ...m, printStatus: next, queuePosition: next === 'printed' ? null : m.queuePosition }
          : m,
      ),
    );
    invoke('set_print_status', { fileId: id, status: next }).catch((e) => {
      console.error('[print-status] Aktualisieren fehlgeschlagen:', e);
    });
  };

  const toggleFavorite = (id: string) => {
    const current = models.find((m) => m.id === id);
    if (!current) return;
    const next = !current.favorite;
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, favorite: next } : m)));
    invoke('set_favorite', { fileId: id, favorite: next }).catch((e) => {
      console.error('[favorite] Aktualisieren fehlgeschlagen:', e);
    });
  };

  const addToQueue = (id: string) => {
    invoke<number>('add_to_queue', { fileId: id })
      .then((position) => {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: position } : m)));
      })
      .catch((e) => console.error('[queue] Hinzufügen fehlgeschlagen:', e));
  };

  const toggleBulkSelect = (id: string) => {
    setSelectedForBulk((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      if (next.size === 0) setConfirmBulkDelete(false);
      return next;
    });
  };

  const selectAllVisible = () => setSelectedForBulk(new Set(filtered.map((m) => m.id)));
  const clearBulkSelection = () => {
    setSelectedForBulk(new Set());
    setConfirmBulkDelete(false);
  };

  const bulkDelete = () => {
    invoke('delete_files', { fileIds: Array.from(selectedForBulk) }).then(() => {
      setModels((prev) => prev.filter((m) => !selectedForBulk.has(m.id)));
      clearBulkSelection();
      setConfirmBulkDelete(false);
      refreshFolders();
      refreshTags();
      refreshCreators();
      refreshTrash();
    });
  };

  const bulkAddToQueue = () => {
    const notYetQueued = models.filter((m) => selectedForBulk.has(m.id) && m.queuePosition === null);
    Promise.all(notYetQueued.map((m) => invoke('add_to_queue', { fileId: m.id }))).then(() =>
      invoke<ModelFile[]>('list_files').then(setModels),
    );
  };

  const bulkSetPrintStatus = (status: 'printed' | 'not_printed') => {
    Promise.all(
      Array.from(selectedForBulk).map((id) => invoke('set_print_status', { fileId: id, status })),
    ).then(() => {
      setModels((prev) =>
        prev.map((m) =>
          selectedForBulk.has(m.id)
            ? { ...m, printStatus: status, queuePosition: status === 'printed' ? null : m.queuePosition }
            : m,
        ),
      );
    });
  };

  const bulkAddToCollection = (collectionId: string) => {
    invoke('add_files_to_collection', { collectionId, fileIds: Array.from(selectedForBulk) }).then(() => {
      refreshCollections();
      if (activeCollection === collectionId) refreshCollectionModels(collectionId);
      clearBulkSelection();
    });
  };

  const addModelToCollection = (fileId: string, collectionId: string) => {
    invoke('add_files_to_collection', { collectionId, fileIds: [fileId] }).then(() => {
      refreshCollections();
      if (activeCollection === collectionId) refreshCollectionModels(collectionId);
    });
  };

  const reorderCollection = (orderedIds: string[]) => {
    if (!activeCollection) return;
    const updates = orderedIds.map((fileId, position) => ({ fileId, position }));
    setCollectionModels((prev) => {
      const byId = new Map(prev.map((m) => [m.id, m]));
      return orderedIds.map((id) => byId.get(id)).filter((m): m is ModelFile => m !== undefined);
    });
    invoke('reorder_collection', { collectionId: activeCollection, updates }).catch((e) => {
      console.error('[collections] Umsortieren fehlgeschlagen:', e);
    });
  };

  const bulkRemoveFromCollection = () => {
    if (!activeCollection) return;
    const ids = Array.from(selectedForBulk);
    Promise.all(
      ids.map((fileId) => invoke('remove_file_from_collection', { collectionId: activeCollection, fileId })),
    ).then(() => {
      refreshCollections();
      refreshCollectionModels(activeCollection);
      clearBulkSelection();
    });
  };

  const removeFromQueue = (id: string) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: null } : m)));
    invoke('remove_from_queue', { fileId: id }).catch((e) => {
      console.error('[queue] Entfernen fehlgeschlagen:', e);
    });
  };

  const reorderQueue = (orderedIds: string[]) => {
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
    invoke('reorder_queue', { updates }).catch((e) => {
      console.error('[queue] Neusortierung fehlgeschlagen:', e);
    });
  };

  const uploadCustomImage = (id: string) => {
    invoke<string | null>('upload_custom_image', { fileId: id })
      .then((customImage) => {
        if (customImage === null) return;
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, customImage } : m)));
      })
      .catch((e) => {
        console.error('[custom-image] Hochladen fehlgeschlagen:', e);
      });
  };

  const captureRenderSnapshot = (id: string, base64: string) => {
    const renderSnapshotImage = `data:image/png;base64,${base64}`;
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, renderSnapshotImage } : m)));
    invoke('set_render_snapshot', { fileId: id, imageBase64: base64 }).catch((e) => {
      console.error('[render-snapshot] Speichern fehlgeschlagen:', e);
    });
  };

  const setModelSourceUrl = (id: string, url: string | null) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, sourceUrl: url } : m)));
    invoke('set_source_url', { fileId: id, url }).catch((e) => {
      console.error('[source-url] Speichern fehlgeschlagen:', e);
    });
  };

  const scanCatalogIssues = () => {
    setCleanupScanning(true);
    setCleanupError(null);
    invoke<CatalogIssues>('scan_catalog_issues')
      .then((issues) => {
        setCleanupIssues(issues);
        setCleanupDialogOpen(true);
      })
      .catch((e) => {
        console.error('[cleanup] Scan fehlgeschlagen:', e);
        setCleanupError(String(e));
      })
      .finally(() => setCleanupScanning(false));
  };

  const deleteSelectedCleanupFiles = (fileIds: string[]) => {
    invoke('delete_files', { fileIds })
      .then(() => {
        // delete_files verarbeitet jede ID einzeln und kann einzelne
        // Eintraege uebersprungen haben (siehe Finding 2 im finalen Review
        // vom 2026-09-10) - daher hier die Modell-Liste komplett neu vom
        // Backend laden statt lokal anhand von fileIds zu filtern, damit die
        // UI auch bei einem teilweise fehlgeschlagenen Batch den wahren
        // DB-Zustand zeigt.
        invoke<ModelFile[]>('list_files').then(setModels);
        setSelectedId((prev) => (prev && fileIds.includes(prev) ? null : prev));
        setCleanupDialogOpen(false);
        setCleanupIssues(null);
        refreshFolders();
        refreshTags();
        refreshCreators();
        refreshTrash();
      })
      .catch((e) => {
        console.error('[cleanup] Löschen fehlgeschlagen:', e);
        setCleanupError(String(e));
      });
  };

  return (
    <div
      className="h-screen min-h-[620px] flex flex-col bg-[var(--bg)] text-[var(--ink)] overflow-hidden"
      style={{ fontSize: 14 }}
    >
      {displayPreference === 'render' && pendingSnapshotIds.length > 0 && (
        <BackgroundSnapshotRenderer
          key={pendingSnapshotIds[0]}
          fileId={pendingSnapshotIds[0]}
          onSnapshotCaptured={(base64) => captureRenderSnapshot(pendingSnapshotIds[0], base64)}
          onError={() => setSkippedSnapshotIds((prev) => new Set(prev).add(pendingSnapshotIds[0]))}
        />
      )}
      <Header
        view={view}
        onViewChange={setView}
        sort={sort}
        onSortChange={setSort}
        hideSortControl={activeCollection !== null}
        count={filtered.length}
        onImportFiles={importFiles}
        onImportFolder={importFolder}
        onImportFolderAsCollection={importFolderAsCollection}
        mainView={mainView}
      />

      <div className="flex-1 flex flex-row min-h-0">
      <Rail
        mainView={mainView}
        onMainViewChange={(v) => {
          setMainView(v);
          setSelectedId(null);
          clearBulkSelection();
          if (v === 'trash') refreshTrash();
        }}
        trashCount={trashModels.length}
        settingsOpen={settingsOpen}
        onSettingsOpenChange={setSettingsOpen}
        themeSetting={setting}
        onThemeChange={setTheme}
        uiDensity={density}
        onUiDensityChange={setDensity}
        displayPreference={displayPreference}
        onDisplayPreferenceChange={setDisplayPreference}
        slicers={slicers}
        onAddSlicer={addSlicer}
        onRemoveSlicer={removeSlicer}
        onScanCatalogIssues={scanCatalogIssues}
        cleanupScanning={cleanupScanning}
        cleanupError={cleanupError}
        onExportCatalog={exportCatalog}
        onImportCatalog={importCatalog}
        catalogBackupError={catalogBackupError}
        catalogBaseDir={catalogBaseDir}
        onOpenCatalogSetup={() => setSetupDialogOpen(true)}
      />
      <div className="flex-1 min-w-0 flex flex-col min-h-0">

      {mainView === 'trash' ? (
        <div className="flex flex-1 min-h-0">
          <main className="flex-1 min-w-0 flex flex-col">
            <div className="flex-none flex items-center justify-between px-4 py-3 border-b border-[var(--line)]">
              <h1 className="text-[15px] font-semibold">{t('trashHeading')}</h1>
              {confirmEmptyTrash ? (
                <div className="flex items-center gap-2">
                  <span className="text-[12.5px] font-medium text-[var(--ink)]">
                    {t('emptyTrashConfirmQuestion')}
                  </span>
                  <button
                    onClick={() => setConfirmEmptyTrash(false)}
                    className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                  >
                    {t('cancel')}
                  </button>
                  <button
                    onClick={emptyTrash}
                    className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
                  >
                    {t('emptyTrashButton')}
                  </button>
                </div>
              ) : (
                <button
                  onClick={() => setConfirmEmptyTrash(true)}
                  disabled={trashModels.length === 0}
                  className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer disabled:opacity-50 hover:border-[var(--accent)] hover:text-[var(--accent)]"
                >
                  {t('emptyTrashButton')}
                </button>
              )}
            </div>
            <div className="flex-1 overflow-y-auto p-4">
              {trashModels.length === 0 ? (
                <p className="font-mono-ui text-[12.5px] text-[var(--ink-3)]">{t('trashEmptyState')}</p>
              ) : view === 'grid' ? (
                <ModelGrid
                  models={trashModels}
                  selectedId={selectedId}
                  onSelect={selectModel}
                  onOpenDetail={() => {}}
                  onContextMenu={() => {}}
                  onToggleFavorite={() => {}}
                  selectedForBulk={new Set()}
                  onToggleBulkSelect={() => {}}
                  displayPreference={displayPreference}
                  readOnly
                />
              ) : (
                <ModelList
                  models={trashModels}
                  selectedId={selectedId}
                  onSelect={selectModel}
                  onOpenDetail={() => {}}
                  onContextMenu={() => {}}
                  selectedForBulk={new Set()}
                  onToggleBulkSelect={() => {}}
                  readOnly
                />
              )}
            </div>
          </main>
          <DetailPanel
            model={trashModels.find((m) => m.id === selectedId) ?? null}
            trashMode
            onRestore={() => selectedId && restoreModel(selectedId)}
            onDeletePermanently={() => selectedId && deleteModelPermanently(selectedId)}
            onAddTag={() => {}}
            onRemoveTag={() => {}}
            onDelete={() => {}}
            onTogglePrintStatus={() => {}}
            onToggleFavorite={() => {}}
            onToggleQueue={() => {}}
            onUploadImage={() => {}}
            onSnapshotCaptured={() => {}}
            onSetSourceUrl={() => {}}
            onOpenInSlicer={() => {}}
            slicers={[]}
            slicerError={null}
          />
        </div>
      ) : mainView === 'catalog' ? (
        <div className="flex-1 flex min-h-0">
          <Sidebar
            query={query}
            onQueryChange={(q) => {
              setQuery(q);
              setActiveCollection(null);
              setCollectionsGalleryOpen(false);
            }}
            queue={queue}
            onQueueReorder={reorderQueue}
            onQueueRemove={removeFromQueue}
            onQueueSelect={selectModel}
            folders={folders}
            totalModelCount={models.length}
            activeFolderId={activeFolderId}
            onFolderSelect={(id) => {
              setActiveFolderId(id);
              setActiveCollection(null);
              setCollectionsGalleryOpen(false);
            }}
            onCreateFolder={onCreateFolder}
            dragOverFolderId={dragOverFolderId}
            onFolderMouseEnter={handleFolderMouseEnter}
            onDragFolderStart={onDragFolderStart}
            tags={tags}
            activeTag={activeTag}
            onTagSelect={(tag) => {
              setActiveTag(tag);
              setActiveCollection(null);
              setCollectionsGalleryOpen(false);
            }}
            collections={collections}
            activeCollection={activeCollection}
            collectionsGalleryOpen={collectionsGalleryOpen}
            onSelectCollection={(id) => {
              setActiveCollection(id);
              setCollectionsGalleryOpen(false);
            }}
            onOpenCollectionsGallery={() => {
              setCollectionsGalleryOpen(true);
              setActiveCollection(null);
            }}
            onCreateCollection={(name) =>
              invoke<Collection>('create_collection', { name }).then(() => refreshCollections())
            }
          />

          <main className="flex-1 min-w-0 flex flex-col min-h-0">
            {activeTag && (
              <div className="flex-none h-[38px] flex items-center gap-2.5 px-4 border-b border-[var(--line)] bg-[var(--bg)]">
                <span
                  onClick={() => setActiveTag(null)}
                  className="flex items-center gap-1.5 h-[22px] px-2 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11px] cursor-pointer"
                >
                  #{activeTag} ✕
                </span>
              </div>
            )}

            {!detailModel && selectedForBulk.size > 0 && (
              <div className="flex-none flex items-center gap-2 px-4 py-2 border-b border-[var(--line)] bg-[var(--panel-2)]">
                {confirmBulkDelete ? (
                  <>
                    <span className="text-[12.5px] font-medium text-[var(--ink)]">
                      {t('bulkDeleteConfirmQuestion').replace('{count}', String(selectedForBulk.size))}
                    </span>
                    <button
                      onClick={() => setConfirmBulkDelete(false)}
                      className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                    >
                      {t('cancel')}
                    </button>
                    <button
                      onClick={bulkDelete}
                      className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
                    >
                      {t('delete')}
                    </button>
                  </>
                ) : (
                  <>
                    <span className="text-[12.5px] font-medium text-[var(--ink)]">
                      {t('bulkSelectedCount').replace('{count}', String(selectedForBulk.size))}
                    </span>
                    <button onClick={selectAllVisible} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('selectAllLabel')}
                    </button>
                    <button onClick={clearBulkSelection} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('clearSelectionLabel')}
                    </button>
                    <span className="flex-1" />
                    <button onClick={bulkAddToQueue} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('addToQueue')}
                    </button>
                    <div className="relative">
                      <button
                        onClick={() => setAddToCollectionMenuOpen((prev) => !prev)}
                        className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]"
                      >
                        {t('addToCollectionLabel')}
                      </button>
                      {addToCollectionMenuOpen && (
                        <div className="absolute top-9 left-0 min-w-[220px] max-w-[360px] py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
                          {collections.map((c) => (
                            <button
                              key={c.id}
                              title={c.name}
                              onClick={() => {
                                bulkAddToCollection(c.id);
                                setAddToCollectionMenuOpen(false);
                              }}
                              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer whitespace-nowrap overflow-hidden text-ellipsis"
                            >
                              {c.name}
                            </button>
                          ))}
                          {collections.length === 0 && (
                            <div className="px-3 py-1.5 font-mono-ui text-[11px] text-[var(--ink-3)]">
                              {t('noCollectionsEmptyState')}
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                    {activeCollection && (
                      <button
                        onClick={bulkRemoveFromCollection}
                        className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]"
                      >
                        {t('removeFromCollectionLabel')}
                      </button>
                    )}
                    <button onClick={() => bulkSetPrintStatus('printed')} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('printedBadge')}
                    </button>
                    <button onClick={() => bulkSetPrintStatus('not_printed')} className="h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:text-[var(--ink)]">
                      {t('notPrintedLabel')}
                    </button>
                    <button onClick={() => setConfirmBulkDelete(true)} className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-red-400 text-[12.5px] font-semibold cursor-pointer hover:border-red-400">
                      {t('delete')}
                    </button>
                  </>
                )}
              </div>
            )}

            {collectionsGalleryOpen ? (
              <CollectionsGallery
                collections={collections}
                onSelect={(id) => {
                  setActiveCollection(id);
                  setCollectionsGalleryOpen(false);
                }}
                onCreate={(name) => invoke<Collection>('create_collection', { name }).then(() => refreshCollections())}
                onRename={(id, name) => invoke('rename_collection', { collectionId: id, name }).then(() => refreshCollections())}
                onDelete={(id) => {
                  invoke('delete_collection', { collectionId: id }).then(() => {
                    refreshCollections();
                    if (activeCollection === id) setActiveCollection(null);
                  });
                }}
              />
            ) : detailModel ? (
              <ModelDetailPage
                model={detailModel}
                onClose={() => setDetailModelId(null)}
                onAddTag={(t) => addTag(detailModel.id, t)}
                onRemoveTag={(t) => removeTag(detailModel.id, t)}
                onDelete={() => {
                  deleteModel(detailModel.id);
                  setDetailModelId(null);
                }}
                onTogglePrintStatus={() => togglePrintStatus(detailModel.id)}
                onToggleFavorite={() => toggleFavorite(detailModel.id)}
                onToggleQueue={() =>
                  detailModel.queuePosition !== null ? removeFromQueue(detailModel.id) : addToQueue(detailModel.id)
                }
                onUploadImage={() => uploadCustomImage(detailModel.id)}
                onSnapshotCaptured={(base64) => captureRenderSnapshot(detailModel.id, base64)}
                onSetSourceUrl={(fileId, url) => setModelSourceUrl(fileId, url)}
                onOpenInSlicer={(slicerId) => openInSlicer(detailModel.id, slicerId)}
                onRescanMetadata={() => rescanMetadata(detailModel.id)}
                onAddToCollection={(collectionId) => addModelToCollection(detailModel.id, collectionId)}
                collections={collections}
                slicers={slicers}
                slicerError={slicerError}
                rescanError={
                  rescanFeedback?.fileId === detailModel.id && rescanFeedback.status === 'error'
                    ? rescanFeedback.message ?? null
                    : null
                }
                rescanSuccess={rescanFeedback?.fileId === detailModel.id && rescanFeedback.status === 'success'}
                displayPreference={displayPreference}
              />
            ) : (
              <div className="flex-1 overflow-y-auto p-4">
                {view === 'grid' ? (
                  <ModelGrid
                    models={activeCollection ? collectionModels : filtered}
                    selectedId={selectedId}
                    onSelect={selectModel}
                    onOpenDetail={setDetailModelId}
                    onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                    onToggleFavorite={toggleFavorite}
                    selectedForBulk={selectedForBulk}
                    onToggleBulkSelect={toggleBulkSelect}
                    displayPreference={displayPreference}
                    reorderable={activeCollection !== null}
                    onReorder={reorderCollection}
                    onDragFileStart={onDragFileStart}
                  />
                ) : (
                  <ModelList
                    models={activeCollection ? collectionModels : filtered}
                    selectedId={selectedId}
                    onSelect={selectModel}
                    onOpenDetail={setDetailModelId}
                    onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                    selectedForBulk={selectedForBulk}
                    onToggleBulkSelect={toggleBulkSelect}
                  />
                )}
              </div>
            )}
          </main>

          {!detailModel && (
            <DetailPanel
              model={selected}
              onAddTag={(t) => selected && addTag(selected.id, t)}
              onRemoveTag={(t) => selected && removeTag(selected.id, t)}
              onDelete={() => selected && deleteModel(selected.id)}
              onTogglePrintStatus={() => selected && togglePrintStatus(selected.id)}
              onToggleFavorite={() => selected && toggleFavorite(selected.id)}
              onToggleQueue={() =>
                selected && (selected.queuePosition !== null ? removeFromQueue(selected.id) : addToQueue(selected.id))
              }
              onUploadImage={() => selected && uploadCustomImage(selected.id)}
              onSnapshotCaptured={(base64) => selected && captureRenderSnapshot(selected.id, base64)}
              onSetSourceUrl={(fileId, url) => setModelSourceUrl(fileId, url)}
              onOpenInSlicer={(slicerId) => selected && openInSlicer(selected.id, slicerId)}
              slicers={slicers}
              slicerError={slicerError}
            />
          )}
        </div>
      ) : (
        <FilamentView />
      )}

      {contextMenu && contextModel && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onOpenInSlicer={() => openInSlicer(contextMenu.modelId)}
          onDelete={() => deleteModel(contextMenu.modelId)}
          inQueue={contextModel.queuePosition !== null}
          onToggleQueue={() =>
            contextModel.queuePosition !== null ? removeFromQueue(contextModel.id) : addToQueue(contextModel.id)
          }
          printed={contextModel.printStatus === 'printed'}
          onTogglePrintStatus={() => togglePrintStatus(contextModel.id)}
        />
      )}

      {importBanner && (
        <ImportSummaryBanner
          imported={importBanner.imported}
          duplicates={importBanner.duplicates}
          onClose={() => setImportBanner(null)}
        />
      )}

      {cleanupDialogOpen && cleanupIssues && (
        <CatalogCleanupDialog
          issues={cleanupIssues}
          onClose={() => setCleanupDialogOpen(false)}
          onDelete={deleteSelectedCleanupFiles}
        />
      )}

      {moveToast && (
        <MoveToast
          from={moveToast.from}
          to={moveToast.to}
          error={moveToast.error}
          onDone={() => setMoveToast(null)}
        />
      )}
      </div>
      </div>
      {setupDialogOpen && (
        <CatalogSetupDialog
          onClose={() => setSetupDialogOpen(false)}
          onLater={() => {
            markSetupSeen();
            setSetupDialogOpen(false);
          }}
          onImported={(result) => {
            markSetupSeen();
            mergeImported(result);
          }}
          onBaseDirSet={(path) => {
            setCatalogBaseDir(path);
            markSetupSeen();
          }}
        />
      )}
    </div>
  );
}
