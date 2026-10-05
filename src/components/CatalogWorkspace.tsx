import { ModelLayoutContext } from '../hooks/ModelLayoutContext';
import { scrollTileIntoView } from '../hooks/useKeyboardShortcuts';
import { shouldIgnoreCatalogShortcut } from '../lib/keyboardGuard';
import { Icon } from './Icon';
import { useLastPrinter } from '../hooks/useLastPrinter';
import { EmptyCatalogTips } from './KeyboardTipsDialog';
import { detailNeighbor, type DetailDirection } from '../hooks/useDetailNavigation';
import { useImportLock } from '../hooks/ImportLockContext';
import type { useFolderExpansion } from '../hooks/useFolderExpansion';
import type { useSidebarWidth } from '../hooks/useSidebarWidth';
import { useMemo, useRef, useLayoutEffect, useEffect, useState, useCallback, useContext, type ReactNode } from 'react';
import { useCatalogScroll } from '../hooks/useCatalogScroll';
import { Sidebar } from './Sidebar';
import { ModelGrid } from './ModelGrid';
import { GroupedModelGrid } from './GroupedModelGrid';
import { GroupedModelList } from './GroupedModelList';
import { DetailPanel } from './DetailPanel';
import { ModelDetailPage } from './ModelDetailPage';
import { CollectionsGallery } from './CollectionsGallery';
import { BulkActionToolbar } from './BulkActionToolbar';
import type { ModelFile, Folder, TagCount, ViewMode, SortKey, Collection, SlicerConfig } from '../types';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import type { useCollapsedFolders } from '../hooks/useCollapsedFolders';
import { useLanguage, useT, useFormatCount } from '../i18n/LanguageContext';
import { tagLabel } from '../lib/autoTags';
import { useFilamentCheck } from '../hooks/useFilamentCheck';
import { selectFromQueue } from '../lib/queueSelect';
import { toolCounts as computeToolCounts, TOOL_VIEW_LABEL_KEY, type ToolView } from '../lib/toolViews';
import type { AppError } from '../lib/errors';

interface CatalogWorkspaceProps {
  detailPanel?: 'auto' | 'pinned';
  onCloseDetails?: () => void;
  printerRefreshKey?: string;
  onOpenTips?: () => void;
  importRow?: ReactNode;
  expansion: ReturnType<typeof useFolderExpansion>;
  catalogKey?: string | null;
  onNavigateDetail?: (direction: DetailDirection) => void;
  sidebarWidth: ReturnType<typeof useSidebarWidth>;
  allFoldersCollapsed: boolean;
  onToggleAllFolders: () => void;
  onClearFilters: () => void;
  query: string;
  setQuery: (q: string) => void;
  setActiveCollection: (id: string | null) => void;
  setCollectionsGalleryOpen: (open: boolean) => void;
  queue: ModelFile[];
  reorderQueue: (orderedIds: string[]) => void;
  removeFromQueue: (id: string) => void;
  selectModel: (id: string) => void;
  folders: Folder[];
  models: ModelFile[];
  activeFolderId: string;
  setActiveFolderId: (id: string) => void;
  onCreateFolder: (parentId: string | null, name: string) => void;
  draggedFileId?: string | null;
  dragOverCollectionId?: string | null;
  handleCollectionMouseEnter?: (id: string) => void;
  handleCollectionMouseLeave?: (id: string) => void;
  dragOverFolderId: string | null;
  draggedFolderId: string | null;
  handleFolderMouseEnter: (id: string) => void;
  handleFolderMouseLeave: (id: string) => void;
  onDragFolderStart: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  setActiveTag: (tag: string | null) => void;
  collections: Collection[];
  activeCollection: string | null;
  collectionsGalleryOpen: boolean;
  createCollection: (name: string) => void;
  renameCollection: (id: string, name: string) => void;
  deleteCollection: (id: string) => void | Promise<void>;
  detailModel: ModelFile | null;
  selectedForBulk: Set<string>;
  confirmBulkDelete: boolean;
  setConfirmBulkDelete: (value: boolean) => void;
  bulkDelete: () => void;
  bulkRemove?: () => Promise<void>;
  onCatalogRemoved?: () => void;
  onRemoveModelFromCatalog?: (id: string) => Promise<void>;
  selectAllVisible: () => void;
  clearBulkSelection: () => void;
  bulkAddToQueue: () => void;
  addToCollectionMenuOpen: boolean;
  setAddToCollectionMenuOpen: (value: boolean) => void;
  bulkAddToCollection: (collectionId: string) => void;
  bulkRemoveFromCollection: () => void;
  bulkSetPrintStatus: (status: 'printed' | 'not_printed') => void;
  addTagMenuOpen: boolean;
  setAddTagMenuOpen: (value: boolean) => void;
  tagDraft: string;
  setTagDraft: (value: string) => void;
  bulkAddTag: () => void;
  removeTagMenuOpen: boolean;
  setRemoveTagMenuOpen: (value: boolean) => void;
  tagsInSelection: string[];
  bulkRemoveTag: (tag: string) => void;
  setDetailModelId: (id: string | null) => void;
  addTag: (id: string, tag: string) => void;
  removeTag: (id: string, tag: string) => void;
  deleteModel: (id: string) => void;
  togglePrintStatus: (id: string) => void;
  toggleFavorite: (id: string) => void;
  addToQueue: (id: string) => void;
  uploadCustomImage: (id: string) => void | Promise<void>;
  captureRenderSnapshot: (id: string, base64: string) => void;
  setModelSourceUrl: (id: string, url: string | null) => void;
  openInSlicer: (id: string) => void;
  rescanMetadata: (id: string) => void;
  addModelToCollection: (fileId: string, collectionId: string) => void;
  slicers: SlicerConfig[];
  slicerError: AppError | null;
  rescanFeedback: { fileId: string; status: 'success' | 'error'; message?: string; unexpected?: boolean } | null;
  displayPreference: DisplayPreference;
  view: ViewMode;
  sort: SortKey;
  filtered: ModelFile[];
  collectionModels: ModelFile[];
  selectedId: string | null;
  setContextMenu: (value: { modelId: string; x: number; y: number } | null) => void;
  toggleBulkSelect: (id: string) => void;
  reorderCollection: (orderedIds: string[]) => void;
  onDragFileStart: (id: string) => void;
  selected: ModelFile | null;
  collapsedFolders: ReturnType<typeof useCollapsedFolders>;
  toolView: ToolView | null;
  setToolView: (v: ToolView | null) => void;
  onOpenCleanup: () => void;
  cleanupScanning: boolean;
  cleanupError: AppError | null;
}

export function CatalogWorkspace({
  detailPanel = 'auto', onCloseDetails, printerRefreshKey = '',
  onOpenTips,
  importRow,
  expansion,
  catalogKey,
  onNavigateDetail,
  sidebarWidth,
  allFoldersCollapsed,
  onToggleAllFolders,
  onClearFilters,
  query,
  setQuery,
  setActiveCollection,
  setCollectionsGalleryOpen,
  queue,
  reorderQueue,
  removeFromQueue,
  selectModel,
  folders,
  models,
  activeFolderId,
  setActiveFolderId,
  onCreateFolder,
  draggedFileId = null,
  dragOverCollectionId,
  handleCollectionMouseEnter,
  handleCollectionMouseLeave,
  dragOverFolderId,
  draggedFolderId,
  handleFolderMouseEnter,
  handleFolderMouseLeave,
  onDragFolderStart,
  tags,
  activeTag,
  setActiveTag,
  collections,
  activeCollection,
  collectionsGalleryOpen,
  createCollection,
  renameCollection,
  deleteCollection,
  detailModel,
  selectedForBulk,
  confirmBulkDelete,
  setConfirmBulkDelete,
  bulkDelete,
  bulkRemove,
  onCatalogRemoved,
  onRemoveModelFromCatalog,
  selectAllVisible,
  clearBulkSelection,
  bulkAddToQueue,
  addToCollectionMenuOpen,
  setAddToCollectionMenuOpen,
  bulkAddToCollection,
  bulkRemoveFromCollection,
  bulkSetPrintStatus,
  addTagMenuOpen,
  setAddTagMenuOpen,
  tagDraft,
  setTagDraft,
  bulkAddTag,
  removeTagMenuOpen,
  setRemoveTagMenuOpen,
  tagsInSelection,
  bulkRemoveTag,
  setDetailModelId,
  addTag,
  removeTag,
  deleteModel,
  togglePrintStatus,
  toggleFavorite,
  addToQueue,
  uploadCustomImage,
  captureRenderSnapshot,
  setModelSourceUrl,
  openInSlicer,
  rescanMetadata,
  addModelToCollection,
  slicers,
  slicerError,
  rescanFeedback,
  displayPreference,
  view,
  sort,
  filtered,
  collectionModels,
  selectedId,
  setContextMenu,
  toggleBulkSelect,
  reorderCollection,
  onDragFileStart,
  selected,
  collapsedFolders,
  toolView,
  setToolView,
  onOpenCleanup,
  cleanupScanning,
  cleanupError,
}: CatalogWorkspaceProps) {
  const [narrow, setNarrow] = useState(() => window.matchMedia?.('(max-width: 999px)').matches ?? false);
  useEffect(() => {
    const media = window.matchMedia?.('(max-width: 999px)');
    if (!media) return;
    const update = () => setNarrow(media.matches);
    update();
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  }, []);
  const layouts = useContext(ModelLayoutContext);
  const panelVisible = !detailModel && (detailPanel === 'pinned' || selectedId !== null);
  useLayoutEffect(() => {
    if (!selectedId || detailModel) return;
    // Let the grid measure its new width before locating the selected row.
    let secondFrame = 0;
    const frame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        const layout = [...(layouts?.layouts.values() ?? [])].find(section => section.order.includes(selectedId));
        layout?.scrollToIndex(layout.order.indexOf(selectedId));
        document.querySelector<HTMLElement>(`[data-model-id="${CSS.escape(selectedId)}"]`)?.scrollIntoView?.({ block: 'nearest' });
      });
    });
    return () => { cancelAnimationFrame(frame); cancelAnimationFrame(secondFrame); };
  }, [panelVisible, narrow, selectedId, detailModel, layouts]);
  const closeDetails = useCallback(() => {
    const tile = selectedId ? document.querySelector<HTMLElement>(`[data-model-id="${CSS.escape(selectedId)}"]`) : null;
    onCloseDetails?.();
    tile?.focus({ preventScroll: true });
    if (selectedId) requestAnimationFrame(() => {
      const layout = [...(layouts?.layouts.values() ?? [])].find(section => section.order.includes(selectedId));
      scrollTileIntoView(selectedId, layout);
    });
  }, [onCloseDetails, selectedId, layouts]);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || detailPanel !== 'auto' || !selectedId || detailModel || selectedForBulk.size > 0 ||
        shouldIgnoreCatalogShortcut(event, { preserveEscapeSelection: true })) return;
      event.preventDefault();
      closeDetails();
    };
    window.addEventListener('keydown', escape);
    return () => window.removeEventListener('keydown', escape);
  }, [detailPanel, selectedId, detailModel, selectedForBulk.size, closeDetails]);
  const containerRef = useRef<HTMLDivElement>(null);
  const savedScroll = useRef(0);
  const wasDetail = useRef(false);
  const resetKey = JSON.stringify([view, activeFolderId, activeTag, activeCollection, toolView, query]);
  const previousResetKey = useRef(resetKey);
  useLayoutEffect(() => {
    const container = containerRef.current;
    const keyChanged = previousResetKey.current !== resetKey;
    // A view or filter change while the detail page is open must not restore the old position.
    if (keyChanged) savedScroll.current = 0;
    if (container) {
      container.scrollTop = keyChanged ? 0 : wasDetail.current ? savedScroll.current : container.scrollTop;
      container.dispatchEvent(new Event('scroll'));
    }
    previousResetKey.current = resetKey;
    wasDetail.current = !!detailModel;
  }, [resetKey, !!detailModel]);
  const { language } = useLanguage();
  const { jobActive } = useImportLock();
  const t = useT();
  const formatCount = useFormatCount();
  const scrollRef = useCatalogScroll(detailModel !== null, JSON.stringify([
    view, sort, language, query, activeFolderId, activeTag, activeCollection, collectionsGalleryOpen, toolView,
  ]), selectedId);
  const refreshKey = `${printerRefreshKey}:${models.map(m => `${m.id}:${m.sliceInfo?.totalWeightG ?? ''}`).join('|')}`;
  const queueFilament = useFilamentCheck(queue.map(m => m.id), refreshKey);
  const selectedFilament = useFilamentCheck(selected ? [selected.id] : [], refreshKey);
  const lastPrinter = useLastPrinter(selected?.id ?? null, printerRefreshKey);
  // Counts over the whole catalog (without folder/tag/search) so they stay stable.
  const counts = useMemo(() => computeToolCounts(models, new Date()), [models]);
  const tagHues = useMemo(() => Object.fromEntries(tags.map(tag => [tag.label, tag.colorHue])), [tags]);
  const displayedModels = activeCollection ? collectionModels : filtered;
  const collectionName = collections.find(c => c.id === activeCollection)?.name ?? activeCollection ?? '';
  const folderName = folders.find(f => f.id === activeFolderId)?.name ?? activeFolderId;
  const chips = [
    ...(toolView ? [{kind: 'filterBarView' as const, value: t(TOOL_VIEW_LABEL_KEY[toolView]),
      removeLabel: t('chipRemove').replace('{label}', t(TOOL_VIEW_LABEL_KEY[toolView])), remove: () => setToolView(null)}] : []),
    ...(activeCollection ? [{kind: 'filterBarCollection' as const, value: collectionName,
      removeLabel: t('filterBarRemoveCollection').replace('{label}', collectionName), remove: () => setActiveCollection(null)}] : []),
    ...(activeFolderId && activeFolderId !== 'all' ? [{kind: 'filterBarFolder' as const, value: folderName,
      removeLabel: t('filterBarRemoveFolder').replace('{label}', folderName), remove: () => setActiveFolderId('all')}] : []),
    ...(activeTag ? [{kind: 'filterBarTag' as const, value: `#${tagLabel(activeTag, language)}`,
      removeLabel: t('chipRemove').replace('{label}', `#${tagLabel(activeTag, language)}`), remove: () => setActiveTag(null)}] : []),
    ...(query ? [{kind: 'filterBarSearch' as const, value: query,
      removeLabel: t('filterBarRemoveSearch').replace('{label}', query), remove: () => setQuery('')}] : []),
  ];
  const clearButtonClass = 'px-2.5 py-1 rounded-[5px] border border-[var(--accent)] text-[var(--accent)] text-[length:var(--font-size-control)] font-semibold hover:bg-[var(--accent-soft)] cursor-pointer';
  const toolOnly = toolView && !activeCollection && activeFolderId === 'all' && !activeTag && !query;
  return (
    <div className="relative flex-1 flex min-h-0">
      <Sidebar
        catalogKey={catalogKey}
        expansion={expansion}
        width={sidebarWidth.width}
        setWidth={sidebarWidth.setWidth}
        resetWidth={sidebarWidth.reset}
        allFoldersCollapsed={allFoldersCollapsed}
        onToggleAllFolders={onToggleAllFolders}
        query={query}
        onQueryChange={(q) => {
          setQuery(q);
          setActiveCollection(null);
          setCollectionsGalleryOpen(false);
        }}
        queue={queue}
        onQueueReorder={reorderQueue}
        onQueueRemove={removeFromQueue}
        onQueueSelect={(id) =>
          selectFromQueue(id, { detailOpen: detailModel !== null, selectModel, openDetail: setDetailModelId })
        }
        queueFilament={queueFilament.checks}
        folders={folders}
        totalModelCount={models.length}
        onCatalogRemoved={onCatalogRemoved}
        activeFolderId={activeFolderId}
        onFolderSelect={(id) => {
          setActiveFolderId(id);
          setActiveCollection(null);
          setCollectionsGalleryOpen(false);
        }}
        onCreateFolder={onCreateFolder}
        dragOverCollectionId={dragOverCollectionId}
        onCollectionMouseEnter={handleCollectionMouseEnter}
        onCollectionMouseLeave={handleCollectionMouseLeave}
        dragOverFolderId={dragOverFolderId}
        draggedFolderId={draggedFolderId}
        draggedFileId={draggedFileId}
        draggedFileFolderId={models.find((model) => model.id === draggedFileId)?.folderId}
        onFolderMouseEnter={handleFolderMouseEnter}
        onFolderMouseLeave={handleFolderMouseLeave}
        onDragFolderStart={jobActive ? () => {} : onDragFolderStart}
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
          setToolView(null);
        }}
        onOpenCollectionsGallery={() => {
          setCollectionsGalleryOpen(true);
          setActiveCollection(null);
          setToolView(null);
        }}
        onCreateCollection={createCollection}
        onRenameCollection={renameCollection}
        onDeleteCollection={async (id) => {
          await deleteCollection(id);
          if (activeCollection === id && !collectionsGalleryOpen) {
            setActiveFolderId('all'); setActiveTag(null); setToolView(null);
          }
        }}
        toolView={toolView}
        onToolViewChange={(v) => {
          setToolView(v);
          // View and collection are mutually exclusive.
          if (v) {
            setActiveCollection(null);
            setCollectionsGalleryOpen(false);
          }
        }}
        toolCounts={counts}
        onOpenCleanup={onOpenCleanup}
        cleanupScanning={cleanupScanning}
        cleanupError={cleanupError}
      />

      <main className="flex-1 min-w-0 flex flex-col min-h-0">
        {importRow}
        {chips.length > 0 && (
          <section aria-label={t('filterBarAria')} className="flex-none flex flex-wrap items-center gap-x-2 gap-y-1.5 px-4 py-2 border-b border-[var(--line)] bg-[var(--bg)]">
            <span className="ui-label text-[var(--ink-3)]">{t('filterBarHeading')}</span>
            {chips.map(chip => (
              <span key={chip.kind} className="inline-flex max-w-full items-center gap-1.5 pl-2 pr-1 py-0.5 rounded-full border border-[var(--line-strong)] bg-[var(--panel)] text-[length:var(--font-size-control)]">
                <span data-filter-kind className="ui-label text-[var(--ink-3)]">{t(chip.kind)}</span>
                <b className="min-w-0 break-words font-semibold">{chip.value}</b>
                <button type="button" onClick={chip.remove} aria-label={chip.removeLabel}
                  className="flex-none w-5 h-5 grid place-items-center rounded-full text-[var(--ink-3)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] cursor-pointer"><Icon name="close" size={14} /></button>
              </span>
            ))}
            <div className="ml-auto flex flex-wrap items-center gap-2">
              <span aria-live="polite" className="font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--ink-2)] tabular-nums">
                {t('filterBarCount').replace('{count}', String(displayedModels.length)).replace('{total}', String(models.length))}
              </span>
              <button type="button" onClick={onClearFilters} className={clearButtonClass}>{t('filterBarClearAll')}</button>
            </div>
          </section>
        )}

        {!detailModel && selectedForBulk.size > 0 && (
          <BulkActionToolbar
            selectedCount={selectedForBulk.size}
            confirmBulkDelete={confirmBulkDelete}
            onConfirmBulkDeleteChange={setConfirmBulkDelete}
            onSelectAllVisible={selectAllVisible}
            onClearSelection={clearBulkSelection}
            onBulkAddToQueue={bulkAddToQueue}
            addToCollectionMenuOpen={addToCollectionMenuOpen}
            onAddToCollectionMenuOpenChange={setAddToCollectionMenuOpen}
            collections={collections}
            onBulkAddToCollection={bulkAddToCollection}
            activeCollection={activeCollection}
            onBulkRemoveFromCollection={bulkRemoveFromCollection}
            onBulkSetPrintStatus={bulkSetPrintStatus}
            onBulkDelete={bulkDelete}
            onBulkRemove={bulkRemove}
            addTagMenuOpen={addTagMenuOpen}
            onAddTagMenuOpenChange={setAddTagMenuOpen}
            tagDraft={tagDraft}
            onTagDraftChange={setTagDraft}
            onSubmitBulkAddTag={bulkAddTag}
            removeTagMenuOpen={removeTagMenuOpen}
            onRemoveTagMenuOpenChange={setRemoveTagMenuOpen}
            tagsInSelection={tagsInSelection}
            onBulkRemoveTag={bulkRemoveTag}
          />
        )}

        {collectionsGalleryOpen ? (
          <CollectionsGallery
            collections={collections}
            onSelect={(id) => {
              setActiveCollection(id);
              setCollectionsGalleryOpen(false);
            }}
            onCreate={createCollection}
            onRename={renameCollection}
            onDelete={deleteCollection}
            onBack={() => setCollectionsGalleryOpen(false)}
          />
        ) : detailModel ? (
          <ModelDetailPage
            key={detailModel.id}
            onNavigate={onNavigateDetail}
            hasPrevious={detailNeighbor(filtered.map(model => model.id), detailModel.id, 'previous') !== null}
            hasNext={detailNeighbor(filtered.map(model => model.id), detailModel.id, 'next') !== null}
            position={filtered.some(model => model.id === detailModel.id)
              ? { index: filtered.findIndex(model => model.id === detailModel.id) + 1, total: filtered.length }
              : undefined}
            allTags={tags.map(tag => tag.label)}
            tagHues={tagHues}
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
            onOpenInSlicer={() => openInSlicer(detailModel.id)}
            onRemoveFromCatalog={onRemoveModelFromCatalog ? () => onRemoveModelFromCatalog(detailModel.id) : undefined}
            onRescanMetadata={() => rescanMetadata(detailModel.id)}
            onAddToCollection={(collectionId) => addModelToCollection(detailModel.id, collectionId)}
            collections={collections}
            slicers={slicers}
            slicerError={slicerError}
            rescanError={
              rescanFeedback?.fileId === detailModel.id && rescanFeedback.status === 'error'
                ? { message: rescanFeedback.message ?? '', unexpected: rescanFeedback.unexpected ?? false }
                : null
            }
            rescanSuccess={rescanFeedback?.fileId === detailModel.id && rescanFeedback.status === 'success'}
            displayPreference={displayPreference}
          />
        ) : (
          <div ref={node => { containerRef.current = node; scrollRef(node); }} data-catalog-scroller onClick={event => {
            const target = event.target;
            if (detailPanel === 'auto' && selectedId && target instanceof Element &&
              !target.closest('[data-model-id], button, input, a, [role="button"], [role="row"], [data-folder-header], [data-model-list-header], [data-navigation-menu]')) closeDetails();
          }} onScroll={event => { savedScroll.current = event.currentTarget.scrollTop; }} className="flex-1 overflow-y-auto overscroll-contain p-4">
            {models.length === 0 && onOpenTips ? <EmptyCatalogTips onOpenTips={onOpenTips} /> : toolOnly && displayedModels.length === 0 ? (
              <div className="font-medium tabular-nums text-[length:var(--font-size-item)] text-[var(--ink-3)] px-1.5 py-8 text-center">
                {t('toolViewEmpty')}
              </div>
            ) : chips.length > 0 && displayedModels.length === 0 ? (
              <div className="flex flex-col items-center gap-2.5 border border-dashed border-[var(--line-strong)] rounded-[10px] px-5 py-8 text-center">
                <h3 className="font-semibold text-[length:var(--font-size-body)]">{t('filterBarEmptyTitle')}</h3>
                <p className="max-w-[48ch] text-[length:var(--font-size-body)] text-[var(--ink-2)]">{formatCount(t('filterBarEmptyText'), models.length)}</p>
                <button type="button" onClick={onClearFilters} className={clearButtonClass}>{t('filterBarClearAll')}</button>
              </div>
            ) : view === 'grid' ? (
              <ModelGrid
                containerRef={containerRef}
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
                onDragFileStart={jobActive ? undefined : onDragFileStart}
              />
            ) : view === 'groupedGrid' ? (
              <GroupedModelGrid
                containerRef={containerRef}
                models={activeCollection ? collectionModels : filtered}
                folders={folders}
                selectedId={selectedId}
                onSelect={selectModel}
                onOpenDetail={setDetailModelId}
                onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                onToggleFavorite={toggleFavorite}
                selectedForBulk={selectedForBulk}
                onToggleBulkSelect={toggleBulkSelect}
                displayPreference={displayPreference}
                onDragFileStart={jobActive ? undefined : onDragFileStart}
                draggedFolderId={draggedFolderId}
                draggedFileId={draggedFileId}
                draggedFileFolderId={models.find((model) => model.id === draggedFileId)?.folderId}
                dragOverFolderId={dragOverFolderId}
                onDragFolderStart={jobActive ? () => {} : onDragFolderStart}
                onFolderMouseEnter={handleFolderMouseEnter}
                onFolderMouseLeave={handleFolderMouseLeave}
                collapsedFolders={collapsedFolders}
              />
            ) : (
              <GroupedModelList
                containerRef={containerRef}
                models={activeCollection ? collectionModels : filtered}
                folders={folders}
                selectedId={selectedId}
                onSelect={selectModel}
                onOpenDetail={setDetailModelId}
                onContextMenu={(id, x, y) => setContextMenu({ modelId: id, x, y })}
                selectedForBulk={selectedForBulk}
                onToggleBulkSelect={toggleBulkSelect}
                onDragFileStart={jobActive ? undefined : onDragFileStart}
                draggedFolderId={draggedFolderId}
                draggedFileId={draggedFileId}
                draggedFileFolderId={models.find((model) => model.id === draggedFileId)?.folderId}
                dragOverFolderId={dragOverFolderId}
                onDragFolderStart={jobActive ? () => {} : onDragFolderStart}
                onFolderMouseEnter={handleFolderMouseEnter}
                onFolderMouseLeave={handleFolderMouseLeave}
                collapsedFolders={collapsedFolders}
              />
            )}
          </div>
        )}
      </main>

      {!detailModel && (detailPanel === 'pinned' || selectedId !== null) && (
        <div role="complementary" aria-label={t('detailPanelTitle')} className={`detail-panel-shell ${narrow ? 'detail-panel-overlay' : ''}`}>
        {detailPanel === 'auto' && <button type="button" className="detail-panel-close" aria-label={t('detailPanelClose')} title={`${t('detailPanelClose')} (Esc)`} onClick={closeDetails}><Icon name="close" size={16} /></button>}
        <DetailPanel
          filament={selectedFilament.error ? null : selectedFilament.checks?.get(selected?.id ?? '')}
          lastPrinter={lastPrinter}
          allTags={tags.map(tag => tag.label)}
          tagHues={tagHues}
          model={selected}
          onAddTag={(t) => selected && addTag(selected.id, t)}
          onRemoveTag={(t) => selected && removeTag(selected.id, t)}
          onDelete={() => selected && deleteModel(selected.id)}
          onTogglePrintStatus={() => selected && togglePrintStatus(selected.id)}
          onToggleFavorite={() => selected && toggleFavorite(selected.id)}
          onToggleQueue={() =>
            selected && (selected.queuePosition !== null ? removeFromQueue(selected.id) : addToQueue(selected.id))
          }
          onUploadImage={() => selected ? uploadCustomImage(selected.id) : undefined}
          onSnapshotCaptured={(base64) => selected && captureRenderSnapshot(selected.id, base64)}
          onSetSourceUrl={(fileId, url) => setModelSourceUrl(fileId, url)}
          onOpenInSlicer={() => selected && openInSlicer(selected.id)}
          hasSlicer={slicers.length > 0}
          onRemoveFromCatalog={selected && onRemoveModelFromCatalog ? () => onRemoveModelFromCatalog(selected.id) : undefined}
          slicerError={slicerError}
        />
        </div>
      )}
    </div>
  );
}
