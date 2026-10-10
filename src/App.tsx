import { NetworkFilesystemWarning } from './components/NetworkFilesystemWarning';
import { KeyboardTipsDialog } from './components/KeyboardTipsDialog';
import { useSingleKeyShortcuts } from './hooks/useSingleKeyShortcuts';
import { ModelLayoutContext } from './hooks/ModelLayoutContext';
import { useModelImages } from './hooks/useModelImages';
import { ImportLockContext } from './hooks/ImportLockContext';
import { PrinterManagerView } from './components/PrinterManagerView';
import type { MainView, PrinterNavigation } from './types';
import { DragGhost } from './components/DragGhost';
import { DragDropTip } from './components/DragDropTip';
import { resolveDisplayImage } from './lib/resolveDisplayImage';
import { useFolderExpansion } from './hooks/useFolderExpansion';
import { detailNeighbor, type DetailDirection } from './hooks/useDetailNavigation';
import { useSidebarWidth } from './hooks/useSidebarWidth';
import { invoke } from '@tauri-apps/api/core';
import { useContext, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { Header } from './components/Header';
import { isWebGLUnavailable } from './lib/webglAvailability';
import { snapshotQueue } from './lib/snapshotQueue';
import { Rail } from './components/Rail';
import { ContextMenu } from './components/ContextMenu';
import { FilamentView } from './components/FilamentView';
import { ImportProgressRow } from './components/ImportProgressRow';
import { ErrorText } from './diagnostics/ErrorText';
import { ImportSummaryBanner } from './components/ImportSummaryBanner';
import { ArchiveImportDialog } from './components/ArchiveImportDialog';
import { CatalogCleanupDialog } from './components/CatalogCleanupDialog';
import { BackgroundSnapshotRenderer } from './components/BackgroundSnapshotRenderer';
import { MoveToast } from './components/MoveToast';
import { CatalogSetupDialog } from './components/CatalogSetupDialog';
import { TrashView } from './components/TrashView';
import { CatalogWorkspace } from './components/CatalogWorkspace';
import { UpdateToast } from './components/UpdateToast';
import { useTheme } from './hooks/useTheme';
import { useDetailPanel } from './hooks/useDetailPanel';
import { useUiDensity } from './hooks/UiDensityContext';
import { useSlicers } from './hooks/useSlicers';
import { useDisplayPreference } from './hooks/useDisplayPreference';
import { useCatalogBaseDir, useRegisterCatalogBaseDirOnStartup } from './hooks/useCatalogBaseDir';
import { useCatalogStore } from './hooks/useCatalogStore';
import { useCatalogFilters } from './hooks/useCatalogFilters';
import { useCollections } from './hooks/useCollections';
import { useFolderDragAndDrop } from './hooks/useFolderDragAndDrop';
import { NO_FOLDER_COLLAPSE_KEY, useCollapsedFolders } from './hooks/useCollapsedFolders';
import { useFileImport } from './hooks/useFileImport';
import { useCatalogBackup } from './hooks/useCatalogBackup';
import { useCatalogCleanup } from './hooks/useCatalogCleanup';
import { useBulkSelection } from './hooks/useBulkSelection';
import { useSlicerLauncher } from './hooks/useSlicerLauncher';
import { useUpdater } from './hooks/useUpdater';
import { useHasStepPreview } from './hooks/useHasStepPreview';
import { useKeyboardShortcuts, scrollTileIntoView } from './hooks/useKeyboardShortcuts';
import { usePrinterLink } from './hooks/usePrinterLink';
import { usePrinters } from './hooks/usePrinters';
import { useLanguage } from './i18n/LanguageContext';

export default function App() {
  const layouts = useContext(ModelLayoutContext);
  const { setting, setTheme } = useTheme();
  const { density, setDensity } = useUiDensity();
  const { detailPanel, setDetailPanel } = useDetailPanel();
  const { slicers, primaryId, addSlicer, addSlicerError, removeSlicer, setPrimary } = useSlicers();
  const { preference: displayPreference, setPreference: setDisplayPreference } = useDisplayPreference();
  const { catalogBaseDir, setCatalogBaseDir, setupSeen, markSetupSeen, resetCatalogSetup } = useCatalogBaseDir();
  const printerLink = usePrinterLink();
  const printers = usePrinters();

  const store = useCatalogStore({ preselectFirst: detailPanel === 'pinned' });
  useRegisterCatalogBaseDirOnStartup(catalogBaseDir, store.refreshFolders);
  const { language } = useLanguage();
  const filters = useCatalogFilters(store.models, store.folders, language);
  const collections = useCollections();
  const dragDrop = useFolderDragAndDrop(store.models, store.folders, {
    refreshFolders: store.refreshFolders,
    refreshFiles: store.refreshFiles,
  }, collections);
  const collapsedFolders = useCollapsedFolders();
  const expansion = useFolderExpansion();
  const sidebarWidth = useSidebarWidth();
  // Models without a folder form their own group in the folder views, so
  // "everything collapsed" has to include that group when it exists.
  const folderIds = new Set(store.folders.map((folder) => folder.id));
  const hasNoFolderGroup = store.models.some((model) => !folderIds.has(model.folderId));
  const allFoldersCollapsed = expansion.expanded.size === 0
    && store.folders.every((folder) => collapsedFolders.isCollapsed(folder.id))
    && (!hasNoFolderGroup || collapsedFolders.isCollapsed(NO_FOLDER_COLLAPSE_KEY));
  const toggleAllFolders = () => {
    if (allFoldersCollapsed) {
      const parents = new Set(store.folders.map((folder) => folder.parentId));
      expansion.setAll(store.folders.filter((folder) => parents.has(folder.id)).map((folder) => folder.id), true);
      collapsedFolders.expandAll();
    } else {
      expansion.setAll([], false);
      collapsedFolders.collapseAll(store.folders.map((folder) => folder.id));
    }
  };

  const [mainView, setMainView] = useState<MainView>('catalog');
  const archiveTargetDefault =
    store.folders.find((f) => f.id === filters.activeFolderId)?.path ?? catalogBaseDir;
  const fileImport = useFileImport({
    enabled: mainView === 'catalog',
    catalogBaseDir,
    activeFolderId: filters.activeFolderId,
    rootFolderId: store.folders.find(f => f.path === catalogBaseDir)?.id,
    targetName: store.folders.find(f => f.id === filters.activeFolderId)?.name ?? catalogBaseDir ?? undefined,
    onImported: store.mergeImported,
    refreshFolders: store.refreshFolders,
    refreshFiles: store.refreshFiles,
  });
  const backup = useCatalogBackup();
  const cleanup = useCatalogCleanup();
  const bulk = useBulkSelection({
    models: store.models,
    setModels: store.setModels,
    refreshFolders: store.refreshFolders,
    refreshTags: store.refreshTags,
    refreshTrash: store.refreshTrash,
    bulkAddToCollection: collections.bulkAddToCollection,
    bulkRemoveFromCollection: collections.bulkRemoveFromCollection,
  });

  const [tipsOpen, setTipsOpen] = useState(false);
  const [singleKeyShortcuts, setSingleKeyShortcuts] = useSingleKeyShortcuts();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const slicerLauncher = useSlicerLauncher(slicers, primaryId, () => setSettingsOpen(true));
  const update = useUpdater();

  const [setupDialogOpen, setSetupDialogOpen] = useState(!setupSeen);
  const [detailModelId, setDetailModelId] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);
  const [confirmEmptyTrash, setConfirmEmptyTrash] = useState(false);

  const refreshAfterRemoval = (includeActiveCollection = true) => {
    store.setSelectedId(null);
    setDetailModelId(null);
    bulk.clearBulkSelection();
    void Promise.all([store.refreshFiles(), store.refreshFolders(), store.refreshTags(),
      store.refreshTrash(), collections.refreshCollections(), printerLink.refresh(),
      ...(includeActiveCollection && collections.activeCollection ? [collections.refreshCollectionModels(collections.activeCollection)] : []),
    ]).catch((e) => console.error('[catalog] refresh after removal failed:', e));
  };
  const removeCatalogModels = async (fileIds: string[]) => {
    await invoke('remove_files_from_catalog', { fileIds });
    store.setModels((models) => models.filter((model) => !fileIds.includes(model.id)));
    refreshAfterRemoval();
  };
  const clearCatalogFilters = () => {
    collections.setActiveCollection(null);
    collections.setCollectionsGalleryOpen(false);
    filters.setActiveFolderId('all');
    filters.setActiveTag(null);
    filters.setQuery('');
    filters.setToolView(null);
  };
  const finishCatalogReset = () => {
    resetCatalogSetup();
    clearCatalogFilters();
    setContextMenu(null);
    setSettingsOpen(false);
    setMainView('catalog');
    store.setModels([]);
    refreshAfterRemoval(false);
    setSetupDialogOpen(true);
  };

  const [printerContext, setPrinterContext] = useState<PrinterNavigation>({});
  const changeMainView = (v: MainView, context: PrinterNavigation = {}) => {
    setPrinterContext(context);
    setMainView(v);
    store.setSelectedId(null);
    bulk.clearBulkSelection();
    if (v === 'trash') store.refreshTrash();
  };

  const dragPointer = useRef<{ x: number; y: number } | null>(null);
  const draggedModel = store.models.find((model) => model.id === dragDrop.draggedFileId);
  const draggedImages = useModelImages(draggedModel ? [draggedModel.id] : []);
  const selected = store.models.find((m) => m.id === store.selectedId) ?? null;
  const detailModel = detailModelId ? store.models.find((m) => m.id === detailModelId) ?? null : null;
  const contextModel = contextMenu ? store.models.find((m) => m.id === contextMenu.modelId) ?? null : null;

  useEffect(() => {
    // Safety net: selectModel normally loads the full data already;
    // ensureFullModel is idempotent.
    if (detailModelId) store.ensureFullModel(detailModelId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detailModelId]);

  const filteredIds = filters.filtered.map(model => model.id);
  const navigateDetail = (direction: DetailDirection) => {
    const id = detailNeighbor(filteredIds, detailModelId, direction);
    if (!id) return;
    store.selectModel(id);
    store.ensureFullModel(id);
    setDetailModelId(id);
  };

  useKeyboardShortcuts({
    singleKeyShortcuts,
    onOpenTips: () => setTipsOpen(true),
    onOpenDetail: setDetailModelId,
    selectAllVisible: () => bulk.selectAllVisible(filteredIds),
    filteredIds,
    selectedId: store.selectedId,
    selectModel: store.selectModel,
    hasBulkSelection: bulk.selectedForBulk.size > 0,
    openBulkDeleteConfirm: () => { if (!fileImport.jobActive) bulk.setConfirmBulkDelete(true); },
    navigationEnabled: mainView === 'catalog' && !detailModelId && !collections.collectionsGalleryOpen,
    toggleBulkSelect: bulk.toggleBulkSelect,
  });

  useEffect(() => {
    // Runs safely after the rename commit, so the tile is already
    // at its new position (see renameFile).
    if (!store.pendingScrollToId) return;
    const id = store.pendingScrollToId;
    scrollTileIntoView(id, [...(layouts?.layouts.values() ?? [])].find(layout => layout.order.includes(id)));
    store.setPendingScrollToId(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [layouts, store.pendingScrollToId]);

  useEffect(() => {
    if (!detailModelId) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (document.querySelector('[role="dialog"][aria-modal="true"]')) return;
      if (e.key !== 'Escape') return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')) return;
      setDetailModelId(null);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [detailModelId]);

  useEffect(() => {
    bulk.clearBulkSelection();
    // Deliberately depends only on filter/collection changes - bulk itself
    // is new on every render and would clear the selection right away.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    filters.activeFolderId,
    filters.activeTag,
    filters.query,
    collections.activeCollection,
    collections.collectionsGalleryOpen,
  ]);

  // Unconfirmed (null) is treated as "no STEP support yet" - one skipped tick
  // for a real STEP-preview build is cheaper than a guaranteed failed attempt.
  const hasStepPreview = useHasStepPreview() ?? false;
  const snapshotIds = useMemo(
    () => snapshotQueue(store.models, store.pendingSnapshotIds, displayPreference, hasStepPreview),
    [store.models, store.pendingSnapshotIds, displayPreference, hasStepPreview],
  );

  const noWebGL = isWebGLUnavailable();
  useEffect(() => {
    // Also skip files imported after WebGL failed, without mounting a viewer.
    if (noWebGL && store.pendingSnapshotIds.length) store.skipSnapshots(store.pendingSnapshotIds);
  }, [noWebGL, store.pendingSnapshotIds, store.skipSnapshots]);

  return (
    <ImportLockContext.Provider value={fileImport.jobActive}>
    <div
      className="h-screen flex flex-col bg-[var(--bg)] text-[var(--ink)] overflow-hidden"
      onMouseMoveCapture={(event) => { dragPointer.current = { x: event.clientX, y: event.clientY }; }}
      style={{ fontSize: 'var(--fs-body)', '--sidebar-width': mainView === 'printers' ? 'var(--pm-master-width)' : `${mainView === 'catalog' ? sidebarWidth.width : 0}px` } as CSSProperties}
    >
      {!noWebGL && snapshotIds.length > 0 && (
        <BackgroundSnapshotRenderer
          key={snapshotIds[0]}
          fileId={snapshotIds[0]}
          onSnapshotCaptured={(base64) => store.captureRenderSnapshot(snapshotIds[0], base64)}
          onError={() => {
            if (isWebGLUnavailable()) store.skipSnapshots(store.pendingSnapshotIds);
            else store.skipSnapshot(snapshotIds[0]);
          }}
        />
      )}
      {tipsOpen && <KeyboardTipsDialog onClose={() => setTipsOpen(false)} />}
      <Header detailPanel={detailPanel} onDetailPanelChange={setDetailPanel}
        onOpenTips={() => setTipsOpen(true)}
        allFoldersCollapsed={allFoldersCollapsed}
        onToggleAllFolders={toggleAllFolders}
        detailOpen={detailModelId !== null}
        view={filters.view}
        onViewChange={filters.setView}
        sort={filters.sort}
        onSortChange={filters.setSort}
        sortDirection={filters.sortDirection}
        onSortDirectionChange={filters.setSortDirection}
        hideSortControl={collections.activeCollection !== null || filters.toolView !== null}
        initialLoading={store.initialLoading}
        count={filters.filtered.length}
        importTargetName={catalogBaseDir ? store.folders.find(f => f.id === filters.activeFolderId)?.name ?? catalogBaseDir : undefined}
        importTargetIsRoot={filters.activeFolderId === 'all'}
        onImportFiles={fileImport.importFiles}
        onImportFolder={fileImport.importFolder}
        mainView={mainView}
      />

      <NetworkFilesystemWarning />
      <div className="flex-1 flex flex-row min-h-0">
        <Rail
          singleKeyShortcuts={singleKeyShortcuts}
          onSingleKeyShortcutsChange={setSingleKeyShortcuts}
          onOpenTips={() => setTipsOpen(true)}
          mainView={mainView}
          onMainViewChange={changeMainView}
          trashCount={store.trashModels.length}
          settingsOpen={settingsOpen}
          onSettingsOpenChange={setSettingsOpen}
          themeSetting={setting}
          onThemeChange={setTheme}
          detailPanel={detailPanel}
          onDetailPanelChange={setDetailPanel}
          uiDensity={density}
          onUiDensityChange={setDensity}
          displayPreference={displayPreference}
          onDisplayPreferenceChange={setDisplayPreference}
          slicers={slicers}
          primarySlicerId={primaryId}
          onAddSlicer={addSlicer}
          addSlicerError={addSlicerError}
          onRemoveSlicer={removeSlicer}
          onSetPrimarySlicer={setPrimary}
          onScanCatalogIssues={cleanup.scanCatalogIssues}
          cleanupScanning={cleanup.cleanupScanning}
          cleanupError={cleanup.cleanupError}
          onExportCatalog={backup.exportCatalog}
          // The backend already uses the new catalog: full reload, otherwise actions
          // with stale IDs would hit the wrong records.
          onImportCatalog={() => backup.importCatalog(() => window.location.reload())}
          catalogBackupError={backup.catalogBackupError}
          onClearCatalogBackupError={backup.clearCatalogBackupError}
          catalogBaseDir={catalogBaseDir}
          catalogModelCount={store.models.length}
          catalogFolderCount={store.folders.length}
          onCatalogReset={finishCatalogReset}
          onOpenCatalogSetup={() => setSetupDialogOpen(true)}
          update={update}
        />
        <div className="relative flex-1 min-w-0 flex flex-col min-h-0">
          {mainView === 'trash' ? (
            <TrashView
              trashModels={store.trashModels}
              selectedId={store.selectedId}
              onSelect={store.selectModel}
              view={filters.view}
              confirmEmptyTrash={confirmEmptyTrash}
              onConfirmEmptyTrashChange={setConfirmEmptyTrash}
              onEmptyTrash={() => store.emptyTrashAction().then(() => setConfirmEmptyTrash(false))}
              onRestore={store.restoreModel}
              onDeletePermanently={store.deleteModelPermanently}
              displayPreference={displayPreference}
            />
          ) : mainView === 'catalog' ? (
            <CatalogWorkspace detailPanel={detailPanel} onCloseDetails={() => store.setSelectedId(null)} printerRefreshKey={String(printerLink.refreshKey)}
              onOpenTips={() => setTipsOpen(true)}
              catalogKey={catalogBaseDir}
              onNavigateDetail={navigateDetail}
              importRow={<>
                {fileImport.progress && fileImport.meta && <ImportProgressRow
                  key={fileImport.jobId}
                  progress={fileImport.progress}
                  meta={fileImport.meta}
                  result={fileImport.result?.jobId === fileImport.jobId ? fileImport.result : null}
                  queued={fileImport.queued}
                  onCancel={fileImport.cancel}
                  onDismiss={fileImport.dismiss}
                  onSelectModel={setDetailModelId}
                />}
                {fileImport.error && <div role="alert"><ErrorText error={fileImport.error} /></div>}
              </>}
              expansion={expansion}
              sidebarWidth={sidebarWidth}
              allFoldersCollapsed={allFoldersCollapsed}
              onToggleAllFolders={toggleAllFolders}
              onClearFilters={clearCatalogFilters}
              query={filters.query}
              setQuery={filters.setQuery}
              setActiveCollection={collections.setActiveCollection}
              setCollectionsGalleryOpen={collections.setCollectionsGalleryOpen}
              queue={filters.queue}
              reorderQueue={store.reorderQueue}
              removeFromQueue={store.removeFromQueue}
              selectModel={store.selectModel}
              folders={store.folders}
              initialLoading={store.initialLoading}
              initialLoadFailed={store.initialLoadFailed}
              models={store.models}
              activeFolderId={filters.activeFolderId}
              setActiveFolderId={filters.setActiveFolderId}
              onCreateFolder={dragDrop.onCreateFolder}
              draggedFileId={dragDrop.draggedFileId}
              dragOverCollectionId={dragDrop.dragOverCollectionId}
              handleCollectionMouseEnter={dragDrop.handleCollectionMouseEnter}
              handleCollectionMouseLeave={dragDrop.handleCollectionMouseLeave}
              dragOverFolderId={dragDrop.dragOverFolderId}
              draggedFolderId={dragDrop.draggedFolderId}
              handleFolderMouseEnter={dragDrop.handleFolderMouseEnter}
              handleFolderMouseLeave={dragDrop.handleFolderMouseLeave}
              onDragFolderStart={dragDrop.onDragFolderStart}
              collapsedFolders={collapsedFolders}
              tags={store.tags}
              activeTag={filters.activeTag}
              setActiveTag={filters.setActiveTag}
              collections={collections.collections}
              activeCollection={collections.activeCollection}
              collectionsGalleryOpen={collections.collectionsGalleryOpen}
              createCollection={collections.createCollection}
              renameCollection={collections.renameCollection}
              deleteCollection={collections.deleteCollection}
              detailModel={detailModel}
              selectedForBulk={bulk.selectedForBulk}
              confirmBulkDelete={bulk.confirmBulkDelete}
              setConfirmBulkDelete={bulk.setConfirmBulkDelete}
              bulkDelete={bulk.bulkDelete}
              bulkRemove={() => removeCatalogModels(Array.from(bulk.selectedForBulk))}
              onCatalogRemoved={refreshAfterRemoval}
              onRemoveModelFromCatalog={(id) => removeCatalogModels([id])}
              selectAllVisible={() => bulk.selectAllVisible(filters.filtered.map((m) => m.id))}
              clearBulkSelection={bulk.clearBulkSelection}
              bulkAddToQueue={bulk.bulkAddToQueue}
              addToCollectionMenuOpen={bulk.addToCollectionMenuOpen}
              setAddToCollectionMenuOpen={bulk.setAddToCollectionMenuOpen}
              bulkAddToCollection={bulk.bulkAddToCollectionAction}
              bulkRemoveFromCollection={bulk.bulkRemoveFromCollectionAction}
              bulkSetPrintStatus={bulk.bulkSetPrintStatus}
              addTagMenuOpen={bulk.addTagMenuOpen}
              setAddTagMenuOpen={bulk.setAddTagMenuOpen}
              tagDraft={bulk.tagDraft}
              setTagDraft={bulk.setTagDraft}
              bulkAddTag={bulk.bulkAddTagAction}
              removeTagMenuOpen={bulk.removeTagMenuOpen}
              setRemoveTagMenuOpen={bulk.setRemoveTagMenuOpen}
              tagsInSelection={bulk.tagsInSelection}
              bulkRemoveTag={bulk.bulkRemoveTagAction}
              setDetailModelId={setDetailModelId}
              addTag={store.addTag}
              removeTag={store.removeTag}
              deleteModel={store.deleteModel}
              togglePrintStatus={store.togglePrintStatus}
              toggleFavorite={store.toggleFavorite}
              addToQueue={store.addToQueue}
              uploadCustomImage={store.uploadCustomImage}
              captureRenderSnapshot={store.captureRenderSnapshot}
              setModelSourceUrl={store.setModelSourceUrl}
              openInSlicer={slicerLauncher.openInSlicer}
              rescanMetadata={store.rescanMetadata}
              addModelToCollection={collections.addModelToCollection}
              slicers={slicers}
              slicerError={slicerLauncher.slicerError}
              rescanFeedback={store.rescanFeedback}
              displayPreference={displayPreference}
              view={filters.view}
              sort={filters.sort}
              filtered={filters.filtered}
              collectionModels={collections.collectionModels}
              collectionLoading={collections.collectionLoading}
              selectedId={store.selectedId}
              setContextMenu={setContextMenu}
              toggleBulkSelect={bulk.toggleBulkSelect}
              reorderCollection={collections.reorderCollection}
              onDragFileStart={dragDrop.onDragFileStart}
              selected={selected}
              toolView={filters.toolView}
              setToolView={filters.setToolView}
              onOpenCleanup={() => void cleanup.scanCatalogIssues()}
              cleanupScanning={cleanup.cleanupScanning}
              cleanupError={cleanup.cleanupError}
            />
          ) : mainView === 'printers' ? (
            <PrinterManagerView printers={printers} printerLink={printerLink} printerId={printerContext.printerId}
              onMaterial={context => changeMainView('filament', context)} />
          ) : (
            <FilamentView printerLink={printerLink} printers={printers} onCatalogChanged={store.refreshFiles}
              printerContext={printerContext} onPrinterManager={printerId => changeMainView('printers', { printerId })} />
          )}

          {contextMenu && contextModel && (
            <ContextMenu
              x={contextMenu.x}
              y={contextMenu.y}
              onClose={() => setContextMenu(null)}
              onOpenInSlicer={() => slicerLauncher.openInSlicer(contextMenu.modelId)}
              onDelete={() => store.deleteModel(contextMenu.modelId)}
              onRemove={() => removeCatalogModels([contextMenu.modelId])}
              inQueue={contextModel.queuePosition !== null}
              onToggleQueue={() =>
                contextModel.queuePosition !== null
                  ? store.removeFromQueue(contextModel.id)
                  : store.addToQueue(contextModel.id)
              }
              printed={contextModel.printStatus === 'printed'}
              onTogglePrintStatus={() => store.togglePrintStatus(contextModel.id)}
              fileId={contextModel.id}
              currentName={contextModel.name}
              onRename={(name) => store.renameFile(contextModel.id, name)}
            />
          )}

          {fileImport.importBanner && (
            <ImportSummaryBanner
              imported={fileImport.importBanner.imported}
              duplicates={fileImport.importBanner.duplicates}
              skipped={fileImport.importBanner.skipped}
              onClose={fileImport.dismissImportBanner}
            />
          )}

          {fileImport.pendingArchives && (
            <ArchiveImportDialog
              archives={fileImport.pendingArchives}
              defaultTargetDir={archiveTargetDefault}
              onCancel={fileImport.cancelArchives}
              onStart={fileImport.startArchives}
            />
          )}

          <UpdateToast view={update} />

          {cleanup.cleanupDialogOpen && cleanup.cleanupIssues && (
            <CatalogCleanupDialog
              issues={cleanup.cleanupIssues}
              onClose={cleanup.closeCleanupDialog}
              onDelete={(fileIds) => cleanup.deleteSelectedCleanupFiles(fileIds, store.refetchAfterPartialDelete)}
            />
          )}

          {mainView === 'catalog' && <DragDropTip modelCount={store.models.length} folderCount={store.folders.length} />}
          {draggedModel && <DragGhost initialPosition={dragPointer.current} key={draggedModel.id} name={draggedModel.name} image={resolveDisplayImage(draggedModel, displayPreference, draggedImages.get(draggedModel.id))} />}
          {dragDrop.moveToast && (
            <MoveToast
              collection={dragDrop.moveToast.collection}
              from={dragDrop.moveToast.from}
              to={dragDrop.moveToast.to}
              error={dragDrop.moveToast.error}
              unexpected={dragDrop.moveToast.unexpected}
              onDone={dragDrop.dismissMoveToast}
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
            fileImport.mergeImported(result);
          }}
          onBaseDirSet={(path) => {
            setCatalogBaseDir(path);
            markSetupSeen();
          }}
        />
      )}
    </div>
    </ImportLockContext.Provider>
  );
}
