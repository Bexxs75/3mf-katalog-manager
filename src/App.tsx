import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ModelGrid } from './components/ModelGrid';
import { ModelList } from './components/ModelList';
import { DetailPanel } from './components/DetailPanel';
import { ContextMenu } from './components/ContextMenu';
import { FilamentView } from './components/FilamentView';
import { ImportSummaryBanner } from './components/ImportSummaryBanner';
import { CatalogCleanupDialog } from './components/CatalogCleanupDialog';
import { useTheme } from './hooks/useTheme';
import { useSlicers } from './hooks/useSlicers';
import type { ModelFile, Folder, TagCount, CreatorCount, CloudAccount, Origin, ViewMode, SortKey, SavedFilter, CatalogIssues } from './types';

interface CloudAccountDto {
  id: string;
  name: string;
  status: 'connected' | 'error' | 'disconnected';
}

interface PickerResultDto {
  cancelled: boolean;
  items: { id: string; name: string; isFolder: boolean }[];
}

interface ImportResultDto {
  imported: ModelFile[];
  duplicateCount: number;
}

// usedPercent/quotaLabel sind noch nicht Teil dieses Backends (echte
// Speicherplatz-Abfrage folgt bei Bedarf spaeter) - "–" statt erfundener
// Zahlen.
const toCloudAccount = (dto: CloudAccountDto): CloudAccount => ({
  id: dto.id as Origin,
  abbr: '',
  name: dto.name,
  status: dto.status,
  usedPercent: 0,
  quotaLabel: '—',
});

export default function App() {
  const { setting, setTheme } = useTheme();
  const { slicers, lastUsedId, addSlicer, removeSlicer, setLastUsed } = useSlicers();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [slicerError, setSlicerError] = useState<string | null>(null);
  const [view, setView] = useState<ViewMode>('grid');
  const [sort, setSort] = useState<SortKey>('name');
  const [query, setQuery] = useState('');
  const [activeFolderId, setActiveFolderId] = useState('all');
  const [activeTag, setActiveTag] = useState<string | null>(null);
  const [creators, setCreators] = useState<CreatorCount[]>([]);
  const [activeCreator, setActiveCreator] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [models, setModels] = useState<ModelFile[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [clouds, setClouds] = useState<CloudAccount[]>([]);
  const [connectingCloud, setConnectingCloud] = useState(false);
  const [cloudError, setCloudError] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);
  const [uploadingId, setUploadingId] = useState<string | null>(null);
  const [cloudUploadError, setCloudUploadError] = useState<string | null>(null);
  const [mainView, setMainView] = useState<'catalog' | 'filament'>('catalog');
  const [importBanner, setImportBanner] = useState<{ imported: number; duplicates: number } | null>(null);
  const [savedFilters, setSavedFilters] = useState<SavedFilter[]>([]);
  const [cleanupDialogOpen, setCleanupDialogOpen] = useState(false);
  const [cleanupIssues, setCleanupIssues] = useState<CatalogIssues | null>(null);
  const [cleanupScanning, setCleanupScanning] = useState(false);
  const [cleanupError, setCleanupError] = useState<string | null>(null);

  const refreshFolders = () => invoke<Folder[]>('list_folders').then(setFolders);
  const refreshTags = () => invoke<TagCount[]>('list_tag_counts').then(setTags);
  const refreshCreators = () => invoke<CreatorCount[]>('list_creators').then(setCreators);
  const refreshSavedFilters = () => invoke<SavedFilter[]>('list_saved_filters').then(setSavedFilters);
  const refreshClouds = () =>
    invoke<CloudAccountDto[]>('list_cloud_accounts').then((accounts) => setClouds(accounts.map(toCloudAccount)));

  // Weitere Anbieter (OneDrive/Dropbox/Proton) haben noch keinen eigenen
  // Connect-Command im Backend - bis dahin verbindet dieser Handler nur
  // Google Drive, unabhaengig von welcher Zeile/welchem "+" er ausgeloest wird.
  const connectCloud = (id: string) => {
    if (connectingCloud || id !== 'gdrive') return;
    setConnectingCloud(true);
    invoke('connect_google_drive')
      .then(() => {
        setCloudError(null);
        refreshClouds();
      })
      .catch((e) => {
        console.error('[cloud] Google Drive verbinden fehlgeschlagen:', e);
        setCloudError(String(e));
      })
      .finally(() => setConnectingCloud(false));
  };

  const handleCloudImport = (fileIds: string[]) => {
    return invoke<ImportResultDto>('import_from_cloud', { fileIds })
      .then(mergeImported)
      .catch((e) => {
        console.error('[cloud] Import aus Google Drive fehlgeschlagen:', e);
        setCloudError(String(e));
      });
  };

  // Datei-/Ordnerauswahl laeuft ueber Googles eigenes Picker-Widget (im
  // System-Browser, siehe cloud::picker im Backend) statt eines eigenen
  // In-App-Dialogs - dafuer genuegt der nicht-sensible drive.file-Scope statt
  // des kostenpflichtige CASA-Audits erfordernden drive.readonly-Scopes.
  const importFromCloud = () => {
    invoke<PickerResultDto>('open_drive_picker', { mode: 'files' })
      .then((result) => {
        if (result.cancelled) return;
        const fileIds = result.items.filter((i) => !i.isFolder).map((i) => i.id);
        if (fileIds.length === 0) return;
        return handleCloudImport(fileIds);
      })
      .catch((e) => {
        console.error('[cloud] Drive-Dateiauswahl fehlgeschlagen:', e);
        setCloudError(String(e));
      });
  };

  const uploadWithFolderPicker = (id: string) => {
    invoke<PickerResultDto>('open_drive_picker', { mode: 'folder' })
      .then((result) => {
        if (result.cancelled) return;
        const folderId = result.items[0]?.id ?? null;
        return uploadToCloud(id, folderId);
      })
      .catch((e) => {
        console.error('[cloud] Drive-Ordnerauswahl fehlgeschlagen:', e);
        setCloudUploadError(String(e));
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
    refreshClouds();
  }, []);

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

  useEffect(() => {
    const model = models.find((m) => m.id === selectedId);
    if (!model || model.origin === 'local') return;
    invoke<string>('check_cloud_sync_status', { fileId: model.id })
      .then((status) => {
        setModels((prev) =>
          prev.map((m) => (m.id === model.id ? { ...m, sync: status as ModelFile['sync'] } : m)),
        );
      })
      .catch((e) => console.error('[cloud] Sync-Check fehlgeschlagen:', e));
  }, [selectedId]);

  const filtered = useMemo(() => {
    return models
      .filter((m) => activeFolderId === 'all' || m.folderId === activeFolderId)
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
  }, [models, activeFolderId, activeTag, activeCreator, query, sort]);

  const queue = useMemo(
    () =>
      models
        .filter((m) => m.queuePosition !== null)
        .sort((a, b) => (a.queuePosition ?? 0) - (b.queuePosition ?? 0)),
    [models],
  );

  const selected = models.find((m) => m.id === selectedId) ?? null;
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

  const importFiles = () => invoke<ImportResultDto>('import_files').then(mergeImported);
  const importFolder = () => invoke<ImportResultDto>('import_folder').then(mergeImported);

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

  const uploadToCloud = (id: string, folderId: string | null) => {
    if (uploadingId) return Promise.resolve();
    setUploadingId(id);
    setCloudUploadError(null);
    return invoke<ModelFile>('upload_file_to_cloud', { fileId: id, folderId })
      .then((updated) => {
        setModels((prev) => prev.map((m) => (m.id === id ? updated : m)));
      })
      .catch((e) => {
        console.error('[cloud] Hochladen fehlgeschlagen:', e);
        setCloudUploadError(String(e));
      })
      .finally(() => setUploadingId(null));
  };

  const deleteModel = (id: string) => {
    invoke('delete_file', { fileId: id }).then(() => {
      setModels((prev) => prev.filter((m) => m.id !== id));
      setSelectedId((prev) => (prev === id ? null : prev));
      refreshFolders();
      refreshTags();
      refreshCreators();
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

  const addToQueue = (id: string) => {
    invoke<number>('add_to_queue', { fileId: id })
      .then((position) => {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: position } : m)));
      })
      .catch((e) => console.error('[queue] Hinzufügen fehlgeschlagen:', e));
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
      .then((displayImage) => {
        if (displayImage === null) return;
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, displayImage } : m)));
      })
      .catch((e) => {
        console.error('[custom-image] Hochladen fehlgeschlagen:', e);
      });
  };

  const captureRenderSnapshot = (id: string, base64: string) => {
    const displayImage = `data:image/png;base64,${base64}`;
    setModels((prev) =>
      prev.map((m) => (m.id === id && m.displayImage === null ? { ...m, displayImage } : m)),
    );
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

  const saveCurrentFilter = (name: string) => {
    invoke<SavedFilter>('save_filter', {
      filter: {
        name,
        folderId: activeFolderId === 'all' ? null : activeFolderId,
        tag: activeTag,
        creator: activeCreator,
        query: query || null,
        sort,
      },
    })
      .then((saved) => setSavedFilters((prev) => [...prev, saved]))
      .catch((e) => console.error('[saved-filter] Speichern fehlgeschlagen:', e));
  };

  const applySavedFilter = (filter: SavedFilter) => {
    setActiveFolderId(filter.folderId ?? 'all');
    setActiveTag(filter.tag);
    setActiveCreator(filter.creator);
    setQuery(filter.query ?? '');
    setSort(filter.sort);
  };

  const deleteSavedFilter = (id: string) => {
    setSavedFilters((prev) => prev.filter((f) => f.id !== id));
    invoke('delete_saved_filter', { filterId: id }).catch((e) => {
      console.error('[saved-filter] Löschen fehlgeschlagen:', e);
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
      <Header
        view={view}
        onViewChange={setView}
        sort={sort}
        onSortChange={setSort}
        count={filtered.length}
        themeSetting={setting}
        onThemeChange={setTheme}
        onImportFiles={importFiles}
        onImportFolder={importFolder}
        cloudDriveConnected={clouds.some((c) => c.id === 'gdrive' && c.status === 'connected')}
        onImportFromCloud={importFromCloud}
        settingsOpen={settingsOpen}
        onSettingsOpenChange={setSettingsOpen}
        slicers={slicers}
        onAddSlicer={addSlicer}
        onRemoveSlicer={removeSlicer}
        onScanCatalogIssues={scanCatalogIssues}
        cleanupScanning={cleanupScanning}
        cleanupError={cleanupError}
        mainView={mainView}
        onMainViewChange={setMainView}
      />

      {mainView === 'catalog' ? (
        <div className="flex-1 flex min-h-0">
          <Sidebar
            query={query}
            onQueryChange={setQuery}
            queue={queue}
            onQueueReorder={reorderQueue}
            onQueueRemove={removeFromQueue}
            onQueueSelect={selectModel}
            folders={folders}
            activeFolderId={activeFolderId}
            onFolderSelect={setActiveFolderId}
            tags={tags}
            activeTag={activeTag}
            onTagSelect={setActiveTag}
            creators={creators}
            activeCreator={activeCreator}
            onCreatorSelect={setActiveCreator}
            savedFilters={savedFilters}
            onSaveFilter={saveCurrentFilter}
            onApplyFilter={applySavedFilter}
            onDeleteFilter={deleteSavedFilter}
            clouds={clouds}
            cloudError={cloudError}
            onAddCloud={() => connectCloud('gdrive')}
            onConnectCloud={connectCloud}
            onDisconnectCloud={(id) => {
              invoke('disconnect_cloud_account', { provider: id })
                .then(() => {
                  setCloudError(null);
                  refreshClouds();
                })
                .catch((e) => {
                  console.error('[cloud] Trennen fehlgeschlagen:', e);
                  setCloudError(String(e));
                });
            }}
          />

          <main className="flex-1 min-w-0 flex flex-col min-h-0">
            <div className="flex-none h-[38px] flex items-center gap-2.5 px-4 border-b border-[var(--line)] bg-[var(--bg)]">
              <span className="font-mono-ui text-[11px] text-[var(--ink-2)]">
                {folders.find((f) => f.id === activeFolderId)?.name}
              </span>
              {activeTag && (
                <span
                  onClick={() => setActiveTag(null)}
                  className="flex items-center gap-1.5 h-[22px] px-2 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11px] cursor-pointer"
                >
                  #{activeTag} ✕
                </span>
              )}
              {activeCreator && (
                <span
                  onClick={() => setActiveCreator(null)}
                  className="flex items-center gap-1.5 h-[22px] px-2 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11px] cursor-pointer"
                >
                  {activeCreator} ✕
                </span>
              )}
            </div>

            <div className="flex-1 overflow-y-auto p-4">
              {view === 'grid' ? (
                <ModelGrid
                  models={filtered}
                  selectedId={selectedId}
                  onSelect={selectModel}
                  onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                />
              ) : (
                <ModelList
                  models={filtered}
                  selectedId={selectedId}
                  onSelect={selectModel}
                  onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                />
              )}
            </div>
          </main>

          <DetailPanel
            model={selected}
            onAddTag={(t) => selected && addTag(selected.id, t)}
            onRemoveTag={(t) => selected && removeTag(selected.id, t)}
            onDelete={() => selected && deleteModel(selected.id)}
            onTogglePrintStatus={() => selected && togglePrintStatus(selected.id)}
            onToggleQueue={() =>
              selected && (selected.queuePosition !== null ? removeFromQueue(selected.id) : addToQueue(selected.id))
            }
            onUploadImage={() => selected && uploadCustomImage(selected.id)}
            onSnapshotCaptured={(base64) => selected && captureRenderSnapshot(selected.id, base64)}
            onSetSourceUrl={(fileId, url) => setModelSourceUrl(fileId, url)}
            onOpenInSlicer={(slicerId) => selected && openInSlicer(selected.id, slicerId)}
            slicers={slicers}
            slicerError={slicerError}
            onUploadToCloud={() => selected && uploadWithFolderPicker(selected.id)}
            cloudUploadAvailable={clouds.some((c) => c.id === 'gdrive' && c.status === 'connected')}
            uploading={uploadingId !== null && uploadingId === selected?.id}
            cloudUploadError={cloudUploadError}
          />
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
    </div>
  );
}
