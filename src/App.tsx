import { useEffect, useState } from 'react';
import { Header } from './components/Header';
import { Rail } from './components/Rail';
import { ContextMenu } from './components/ContextMenu';
import { FilamentView } from './components/FilamentView';
import { ImportSummaryBanner } from './components/ImportSummaryBanner';
import { ArchiveImportDialog } from './components/ArchiveImportDialog';
import { CatalogCleanupDialog } from './components/CatalogCleanupDialog';
import { BackgroundSnapshotRenderer } from './components/BackgroundSnapshotRenderer';
import { MoveToast } from './components/MoveToast';
import { CatalogSetupDialog } from './components/CatalogSetupDialog';
import { TrashView } from './components/TrashView';
import { CatalogWorkspace } from './components/CatalogWorkspace';
import { UpdateAvailableToast } from './components/UpdateAvailableToast';
import { useTheme } from './hooks/useTheme';
import { useUiDensity } from './hooks/UiDensityContext';
import { useSlicers } from './hooks/useSlicers';
import { useDisplayPreference } from './hooks/useDisplayPreference';
import { useCatalogBaseDir, useRegisterCatalogBaseDirOnStartup } from './hooks/useCatalogBaseDir';
import { useCatalogStore } from './hooks/useCatalogStore';
import { useCatalogFilters } from './hooks/useCatalogFilters';
import { useCollections } from './hooks/useCollections';
import { useFolderDragAndDrop } from './hooks/useFolderDragAndDrop';
import { useCollapsedFolders } from './hooks/useCollapsedFolders';
import { useFileImport } from './hooks/useFileImport';
import { useCatalogBackup } from './hooks/useCatalogBackup';
import { useCatalogCleanup } from './hooks/useCatalogCleanup';
import { useBulkSelection } from './hooks/useBulkSelection';
import { useSlicerLauncher } from './hooks/useSlicerLauncher';
import { useUpdateCheck } from './hooks/useUpdateCheck';
import { useKeyboardShortcuts, MODEL_TILE_ATTR } from './hooks/useKeyboardShortcuts';
import { usePrinterLink } from './hooks/usePrinterLink';
import { usePrinters } from './hooks/usePrinters';
import { useLanguage } from './i18n/LanguageContext';

export default function App() {
  const { setting, setTheme } = useTheme();
  const { density, setDensity } = useUiDensity();
  const { slicers, primaryId, addSlicer, addSlicerError, removeSlicer, setPrimary } = useSlicers();
  const { preference: displayPreference, setPreference: setDisplayPreference } = useDisplayPreference();
  const { catalogBaseDir, setCatalogBaseDir, setupSeen, markSetupSeen } = useCatalogBaseDir();
  const printerLink = usePrinterLink();
  const printers = usePrinters();

  const store = useCatalogStore();
  useRegisterCatalogBaseDirOnStartup(catalogBaseDir, store.refreshFolders);
  const { language } = useLanguage();
  const filters = useCatalogFilters(store.models, store.folders, language);
  const collections = useCollections();
  const dragDrop = useFolderDragAndDrop(store.models, store.folders, {
    refreshFolders: store.refreshFolders,
    refreshFiles: store.refreshFiles,
  });
  const collapsedFolders = useCollapsedFolders();

  const [mainView, setMainView] = useState<'catalog' | 'filament' | 'trash'>('catalog');
  const archiveTargetDefault =
    store.folders.find((f) => f.id === filters.activeFolderId)?.path ?? catalogBaseDir;
  const fileImport = useFileImport({
    enabled: mainView === 'catalog',
    catalogBaseDir,
    activeFolderId: filters.activeFolderId,
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
    refreshCreators: store.refreshCreators,
    refreshTrash: store.refreshTrash,
    bulkAddToCollection: collections.bulkAddToCollection,
    bulkRemoveFromCollection: collections.bulkRemoveFromCollection,
  });

  const [settingsOpen, setSettingsOpen] = useState(false);
  const slicerLauncher = useSlicerLauncher(slicers, primaryId, () => setSettingsOpen(true));
  const update = useUpdateCheck();

  const [setupDialogOpen, setSetupDialogOpen] = useState(!setupSeen);
  const [detailModelId, setDetailModelId] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ modelId: string; x: number; y: number } | null>(null);
  const [confirmEmptyTrash, setConfirmEmptyTrash] = useState(false);

  const changeMainView = (v: 'catalog' | 'filament' | 'trash') => {
    setMainView(v);
    store.setSelectedId(null);
    bulk.clearBulkSelection();
    if (v === 'trash') store.refreshTrash();
  };

  const selected = store.models.find((m) => m.id === store.selectedId) ?? null;
  const detailModel = detailModelId ? store.models.find((m) => m.id === detailModelId) ?? null : null;
  const contextModel = contextMenu ? store.models.find((m) => m.id === contextMenu.modelId) ?? null : null;

  useEffect(() => {
    // Sicherheitsnetz fuer Finding M-01: `selectModel` (immer vor einem
    // Doppelklick ausgeloest) stoesst das Nachladen der vollen Modelldaten
    // bereits an, aber falls die Detailseite jemals ohne vorherigen
    // selectModel()-Aufruf geoeffnet wird, holt dieser Effekt die vollen
    // Daten trotzdem nach (ensureFullModel ist idempotent).
    if (detailModelId) store.ensureFullModel(detailModelId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detailModelId]);

  useKeyboardShortcuts({
    filteredIds: filters.filtered.map((m) => m.id),
    selectedId: store.selectedId,
    selectModel: store.selectModel,
    hasBulkSelection: bulk.selectedForBulk.size > 0,
    openBulkDeleteConfirm: () => bulk.setConfirmBulkDelete(true),
    navigationEnabled: mainView === 'catalog' && !detailModelId && !collections.collectionsGalleryOpen,
    toggleBulkSelect: bulk.toggleBulkSelect,
  });

  useEffect(() => {
    // Laeuft garantiert erst NACH dem Commit+Paint des Re-Renders, der durch
    // renameFile()s setModels() ausgeloest wurde (siehe Kommentar dort) -
    // anders als ein rohes requestAnimationFrame direkt im Promise-Handler
    // ist hier sichergestellt, dass die Kachel bereits an ihrer neuen,
    // sortierten Position im DOM sitzt, bevor gescrollt wird.
    if (!store.pendingScrollToId) return;
    const id = store.pendingScrollToId;
    document.querySelector(`[${MODEL_TILE_ATTR}="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    store.setPendingScrollToId(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [store.pendingScrollToId]);

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
    bulk.clearBulkSelection();
    // Absichtlich nur an den Filter-/Sammlungs-Wechseln haengend - bulk selbst
    // ist bei jedem Render neu, wuerde die Auswahl also sofort wieder leeren.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    filters.activeFolderId,
    filters.activeTag,
    filters.activeCreator,
    filters.query,
    collections.activeCollection,
    collections.collectionsGalleryOpen,
  ]);

  return (
    <div
      className="h-screen min-h-[620px] flex flex-col bg-[var(--bg)] text-[var(--ink)] overflow-hidden"
      style={{ fontSize: 14 }}
    >
      {displayPreference === 'render' && store.pendingSnapshotIds.length > 0 && (
        <BackgroundSnapshotRenderer
          key={store.pendingSnapshotIds[0]}
          fileId={store.pendingSnapshotIds[0]}
          onSnapshotCaptured={(base64) => store.captureRenderSnapshot(store.pendingSnapshotIds[0], base64)}
          onError={() => store.skipSnapshot(store.pendingSnapshotIds[0])}
        />
      )}
      <Header
        view={filters.view}
        onViewChange={filters.setView}
        sort={filters.sort}
        onSortChange={filters.setSort}
        hideSortControl={collections.activeCollection !== null || filters.toolView !== null}
        count={filters.filtered.length}
        onImportFiles={fileImport.importFiles}
        onImportFolder={fileImport.importFolder}
        mainView={mainView}
      />

      <div className="flex-1 flex flex-row min-h-0">
        <Rail
          mainView={mainView}
          onMainViewChange={changeMainView}
          trashCount={store.trashModels.length}
          settingsOpen={settingsOpen}
          onSettingsOpenChange={setSettingsOpen}
          themeSetting={setting}
          onThemeChange={setTheme}
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
          // Backend hat AppState.db bereits auf den neu importierten Katalog
          // umverbunden - ohne sofortigen Reload wuerde das Frontend weiter
          // veraltete Modell-IDs aus dem alten Katalog anzeigen und Aktionen
          // (Loeschen/Favorit/Tag) koennten versehentlich falsche Datensaetze
          // im neuen Katalog treffen (Finding C1). Ein voller Reload laedt die
          // React-App komplett neu und holt alle Daten gegen die jetzt aktive
          // DB neu ab.
          onImportCatalog={() => backup.importCatalog(() => window.location.reload())}
          catalogBackupError={backup.catalogBackupError}
          catalogBaseDir={catalogBaseDir}
          onOpenCatalogSetup={() => setSetupDialogOpen(true)}
          printerLink={printerLink}
          printerList={printers.printers}
          updateInfo={{
            currentVersion: update.currentVersion,
            latestVersion: update.latestVersion,
            updateAvailable: update.updateAvailable,
            checking: update.checking,
            checkNow: update.checkNow,
            download: update.download,
          }}
        />
        <div className="flex-1 min-w-0 flex flex-col min-h-0">
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
            <CatalogWorkspace
              query={filters.query}
              setQuery={filters.setQuery}
              setActiveCollection={collections.setActiveCollection}
              setCollectionsGalleryOpen={collections.setCollectionsGalleryOpen}
              queue={filters.queue}
              reorderQueue={store.reorderQueue}
              removeFromQueue={store.removeFromQueue}
              selectModel={store.selectModel}
              folders={store.folders}
              models={store.models}
              activeFolderId={filters.activeFolderId}
              setActiveFolderId={filters.setActiveFolderId}
              onCreateFolder={dragDrop.onCreateFolder}
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
              filtered={filters.filtered}
              collectionModels={collections.collectionModels}
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
          ) : (
            <FilamentView printerLink={printerLink} printers={printers} />
          )}

          {contextMenu && contextModel && (
            <ContextMenu
              x={contextMenu.x}
              y={contextMenu.y}
              onClose={() => setContextMenu(null)}
              onOpenInSlicer={() => slicerLauncher.openInSlicer(contextMenu.modelId)}
              onDelete={() => store.deleteModel(contextMenu.modelId)}
              inQueue={contextModel.queuePosition !== null}
              onToggleQueue={() =>
                contextModel.queuePosition !== null
                  ? store.removeFromQueue(contextModel.id)
                  : store.addToQueue(contextModel.id)
              }
              printed={contextModel.printStatus === 'printed'}
              onTogglePrintStatus={() => store.togglePrintStatus(contextModel.id)}
              currentName={contextModel.name}
              onRename={(name) => store.renameFile(contextModel.id, name)}
            />
          )}

          {fileImport.importBanner && (
            <ImportSummaryBanner
              imported={fileImport.importBanner.imported}
              duplicates={fileImport.importBanner.duplicates}
              archives={fileImport.importBanner.archives}
              onClose={fileImport.dismissImportBanner}
            />
          )}

          {fileImport.pendingArchives && (
            <ArchiveImportDialog
              archives={fileImport.pendingArchives}
              defaultTargetDir={archiveTargetDefault}
              onCancel={fileImport.cancelArchives}
              onDone={fileImport.finishArchives}
            />
          )}

          {update.updateAvailable && !update.dismissed && (
            <UpdateAvailableToast
              latestVersion={update.latestVersion}
              onDownload={update.download}
              onDismiss={update.dismiss}
            />
          )}

          {cleanup.cleanupDialogOpen && cleanup.cleanupIssues && (
            <CatalogCleanupDialog
              issues={cleanup.cleanupIssues}
              onClose={cleanup.closeCleanupDialog}
              onDelete={(fileIds) => cleanup.deleteSelectedCleanupFiles(fileIds, store.refetchAfterPartialDelete)}
            />
          )}

          {dragDrop.moveToast && (
            <MoveToast
              from={dragDrop.moveToast.from}
              to={dragDrop.moveToast.to}
              error={dragDrop.moveToast.error}
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
  );
}
