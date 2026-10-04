import { useImportLock } from '../hooks/ImportLockContext';
import type { useFolderExpansion } from '../hooks/useFolderExpansion';
import type { useSidebarWidth } from '../hooks/useSidebarWidth';
import { useMemo, type ReactNode } from 'react';
import { Sidebar } from './Sidebar';
import { ModelGrid } from './ModelGrid';
import { GroupedModelGrid } from './GroupedModelGrid';
import { GroupedModelList } from './GroupedModelList';
import { DetailPanel } from './DetailPanel';
import { ModelDetailPage } from './ModelDetailPage';
import { CollectionsGallery } from './CollectionsGallery';
import { BulkActionToolbar } from './BulkActionToolbar';
import type { ModelFile, Folder, TagCount, ViewMode, Collection, SlicerConfig } from '../types';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import type { useCollapsedFolders } from '../hooks/useCollapsedFolders';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { tagLabel } from '../lib/autoTags';
import { useFilamentCheck } from '../hooks/useFilamentCheck';
import { selectFromQueue } from '../lib/queueSelect';
import { toolCounts as computeToolCounts, TOOL_VIEW_LABEL_KEY, type ToolView } from '../lib/toolViews';
import type { AppError } from '../lib/errors';

interface CatalogWorkspaceProps {
  importRow?: ReactNode;
  expansion: ReturnType<typeof useFolderExpansion>;
  sidebarWidth: ReturnType<typeof useSidebarWidth>;
  allFoldersCollapsed: boolean;
  onToggleAllFolders: () => void;
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
  deleteCollection: (id: string) => void;
  detailModel: ModelFile | null;
  selectedForBulk: Set<string>;
  confirmBulkDelete: boolean;
  setConfirmBulkDelete: (value: boolean) => void;
  bulkDelete: () => void;
  bulkRemove?: () => Promise<void>;
  onCatalogRemoved?: () => void;
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
  uploadCustomImage: (id: string) => void;
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
  importRow,
  expansion,
  sidebarWidth,
  allFoldersCollapsed,
  onToggleAllFolders,
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
  const { language } = useLanguage();
  const { jobActive } = useImportLock();
  const t = useT();
  const queueFilament = useFilamentCheck(
    queue.map((m) => m.id),
    // Forces a reload when the slicer data of a queue entry changes
    // (e.g. after "Re-read metadata"), even if the IDs and their
    // order stay the same.
    queue.map((m) => `${m.id}:${m.sliceInfo?.totalWeightG ?? ''}`).join('|'),
  );
  // Counts over the whole catalog (without folder/tag/search) so they stay stable.
  const counts = useMemo(() => computeToolCounts(models, new Date()), [models]);
  return (
    <div className="flex-1 flex min-h-0">
      <Sidebar
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
        {(activeTag || toolView) && (
          <div className="flex-none h-[38px] flex items-center gap-2.5 px-4 border-b border-[var(--line)] bg-[var(--bg)]">
            {toolView && (
              <button
                type="button"
                onClick={() => setToolView(null)}
                aria-label={t('chipRemove').replace('{label}', t(TOOL_VIEW_LABEL_KEY[toolView]))}
                className="flex items-center gap-1.5 h-[22px] px-2 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11px] cursor-pointer"
              >
                {t(TOOL_VIEW_LABEL_KEY[toolView])} ✕
              </button>
            )}
            {activeTag && (
              <button
                type="button"
                onClick={() => setActiveTag(null)}
                aria-label={t('chipRemove').replace('{label}', `#${tagLabel(activeTag, language)}`)}
                className="flex items-center gap-1.5 h-[22px] px-2 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11px] cursor-pointer"
              >
                #{tagLabel(activeTag, language)} ✕
              </button>
            )}
          </div>
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
          />
        ) : detailModel ? (
          <ModelDetailPage
            allTags={tags.map(tag => tag.label)}
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
          <div className="flex-1 overflow-y-auto overscroll-contain p-4">
            {toolView && filtered.length === 0 && !activeCollection ? (
              <div className="font-mono-ui text-[length:var(--font-size-item)] text-[var(--ink-3)] px-1.5 py-8 text-center">
                {t('toolViewEmpty')}
              </div>
            ) : view === 'grid' ? (
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
                onDragFileStart={jobActive ? undefined : onDragFileStart}
              />
            ) : view === 'groupedGrid' ? (
              <GroupedModelGrid
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

      {!detailModel && (
        <DetailPanel
          allTags={tags.map(tag => tag.label)}
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
          onOpenInSlicer={() => selected && openInSlicer(selected.id)}
          slicerError={slicerError}
        />
      )}
    </div>
  );
}
