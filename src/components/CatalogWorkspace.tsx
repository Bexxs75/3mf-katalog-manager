import { invoke } from '@tauri-apps/api/core';
import { Sidebar } from './Sidebar';
import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
import { DetailPanel } from './DetailPanel';
import { ModelDetailPage } from './ModelDetailPage';
import { CollectionsGallery } from './CollectionsGallery';
import { BulkActionToolbar } from './BulkActionToolbar';
import type { ModelFile, Folder, TagCount, ViewMode, Collection, SlicerConfig } from '../types';
import type { DisplayPreference } from '../hooks/useDisplayPreference';

interface CatalogWorkspaceProps {
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
  dragOverFolderId: string | null;
  handleFolderMouseEnter: (id: string) => void;
  onDragFolderStart: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  setActiveTag: (tag: string | null) => void;
  collections: Collection[];
  activeCollection: string | null;
  collectionsGalleryOpen: boolean;
  refreshCollections: () => void;
  detailModel: ModelFile | null;
  selectedForBulk: Set<string>;
  confirmBulkDelete: boolean;
  setConfirmBulkDelete: (value: boolean) => void;
  bulkDelete: () => void;
  selectAllVisible: () => void;
  clearBulkSelection: () => void;
  bulkAddToQueue: () => void;
  addToCollectionMenuOpen: boolean;
  setAddToCollectionMenuOpen: (value: boolean) => void;
  bulkAddToCollection: (collectionId: string) => void;
  bulkRemoveFromCollection: () => void;
  bulkSetPrintStatus: (status: 'printed' | 'not_printed') => void;
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
  slicerError: string | null;
  rescanFeedback: { fileId: string; status: 'success' | 'error'; message?: string } | null;
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
}

export function CatalogWorkspace({
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
  dragOverFolderId,
  handleFolderMouseEnter,
  onDragFolderStart,
  tags,
  activeTag,
  setActiveTag,
  collections,
  activeCollection,
  collectionsGalleryOpen,
  refreshCollections,
  detailModel,
  selectedForBulk,
  confirmBulkDelete,
  setConfirmBulkDelete,
  bulkDelete,
  selectAllVisible,
  clearBulkSelection,
  bulkAddToQueue,
  addToCollectionMenuOpen,
  setAddToCollectionMenuOpen,
  bulkAddToCollection,
  bulkRemoveFromCollection,
  bulkSetPrintStatus,
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
}: CatalogWorkspaceProps) {
  return (
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
          />
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
            onOpenInSlicer={() => openInSlicer(detailModel.id)}
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
          <div className="flex-1 overflow-y-auto overscroll-contain p-4">
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
          onOpenInSlicer={() => selected && openInSlicer(selected.id)}
          slicerError={slicerError}
        />
      )}
    </div>
  );
}
