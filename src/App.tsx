import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Header } from './components/Header';
import { Sidebar } from './components/Sidebar';
import { ModelGrid } from './components/ModelGrid';
import { ModelList } from './components/ModelList';
import { DetailPanel } from './components/DetailPanel';
import { ModelDetailPage } from './components/ModelDetailPage';
import { ContextMenu } from './components/ContextMenu';
import { FilamentView } from './components/FilamentView';
import { ImportSummaryBanner } from './components/ImportSummaryBanner';
import { CatalogCleanupDialog } from './components/CatalogCleanupDialog';
import { useTheme } from './hooks/useTheme';
import { useUiDensity } from './hooks/UiDensityContext';
import { useSlicers } from './hooks/useSlicers';
import { useT } from './i18n/LanguageContext';
import type { ModelFile, Folder, TagCount, CreatorCount, ViewMode, SortKey, SavedFilter, CatalogIssues } from './types';

interface ImportResultDto {
  imported: ModelFile[];
  duplicateCount: number;
}

export default function App() {
  const { setting, setTheme } = useTheme();
  const t = useT();
  const { density, setDensity } = useUiDensity();
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
  const [selectedForBulk, setSelectedForBulk] = useState<Set<string>>(new Set());
  // @ts-expect-error confirmBulkDelete wird erst in Task 3 im JSX verwendet
  const [confirmBulkDelete, setConfirmBulkDelete] = useState(false);
  const [detailModelId, setDetailModelId] = useState<string | null>(null);
  const [models, setModels] = useState<ModelFile[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);
  const [mainView, setMainView] = useState<'catalog' | 'filament' | 'trash'>('catalog');
  const [trashModels, setTrashModels] = useState<ModelFile[]>([]);
  const [confirmEmptyTrash, setConfirmEmptyTrash] = useState(false);
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
  const refreshTrash = () => invoke<ModelFile[]>('list_trash').then(setTrashModels);

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
  }, []);

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
  }, [activeFolderId, activeTag, activeCreator, query]);

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

  // @ts-expect-error toggleBulkSelect wird erst in Task 3 im JSX verwendet
  const toggleBulkSelect = (id: string) => {
    setSelectedForBulk((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  // @ts-expect-error selectAllVisible wird erst in Task 3 im JSX verwendet
  const selectAllVisible = () => setSelectedForBulk(new Set(filtered.map((m) => m.id)));
  const clearBulkSelection = () => setSelectedForBulk(new Set());

  // @ts-expect-error bulkDelete wird erst in Task 3 im JSX verwendet
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

  // @ts-expect-error bulkAddToQueue wird erst in Task 3 im JSX verwendet
  const bulkAddToQueue = () => {
    Promise.all(Array.from(selectedForBulk).map((id) => invoke('add_to_queue', { fileId: id }))).then(
      () => invoke<ModelFile[]>('list_files').then(setModels),
    );
  };

  // @ts-expect-error bulkSetPrintStatus wird erst in Task 3 im JSX verwendet
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
      <Header
        view={view}
        onViewChange={setView}
        sort={sort}
        onSortChange={setSort}
        count={filtered.length}
        themeSetting={setting}
        onThemeChange={setTheme}
        uiDensity={density}
        onUiDensityChange={setDensity}
        onImportFiles={importFiles}
        onImportFolder={importFolder}
        settingsOpen={settingsOpen}
        onSettingsOpenChange={setSettingsOpen}
        slicers={slicers}
        onAddSlicer={addSlicer}
        onRemoveSlicer={removeSlicer}
        onScanCatalogIssues={scanCatalogIssues}
        cleanupScanning={cleanupScanning}
        cleanupError={cleanupError}
        mainView={mainView}
        onMainViewChange={(v) => {
          setMainView(v);
          setSelectedId(null);
          if (v === 'trash') refreshTrash();
        }}
        trashCount={trashModels.length}
      />

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
                  readOnly
                />
              ) : (
                <ModelList
                  models={trashModels}
                  selectedId={selectedId}
                  onSelect={selectModel}
                  onOpenDetail={() => {}}
                  onContextMenu={() => {}}
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

            {detailModel ? (
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
                slicers={slicers}
                slicerError={slicerError}
              />
            ) : (
              <div className="flex-1 overflow-y-auto p-4">
                {view === 'grid' ? (
                  <ModelGrid
                    models={filtered}
                    selectedId={selectedId}
                    onSelect={selectModel}
                    onOpenDetail={setDetailModelId}
                    onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                    onToggleFavorite={toggleFavorite}
                  />
                ) : (
                  <ModelList
                    models={filtered}
                    selectedId={selectedId}
                    onSelect={selectModel}
                    onOpenDetail={setDetailModelId}
                    onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
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
