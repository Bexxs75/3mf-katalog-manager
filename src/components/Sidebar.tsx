import { useImportLock } from '../hooks/ImportLockContext';
import type { useFolderExpansion } from '../hooks/useFolderExpansion';
import { SIDEBAR_MIN, SIDEBAR_MAX, SIDEBAR_STEP } from '../hooks/useSidebarWidth';
import { ContextMenu } from './ContextMenu';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { messageOf } from '../lib/errors';
import { Icon } from './Icon';
import { TagDot } from './TagDot';
import { useEffect, useRef, useState } from 'react';
import type { Folder, TagCount, ModelFile, Collection, FilamentCheck } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { SEARCH_INPUT_ID } from '../hooks/useKeyboardShortcuts';
import { FolderTree } from './FolderTree';
import { sortTagsForDisplay, tagLabel, tagMatches } from '../lib/autoTags';
import { ToolsSection } from './ToolsSection';
import type { ToolCounts, ToolView } from '../lib/toolViews';
import type { AppError } from '../lib/errors';

interface Props {
  catalogKey?: string | null;
  expansion: ReturnType<typeof useFolderExpansion>;
  allFoldersCollapsed: boolean;
  onToggleAllFolders: () => void;
  width: number;
  setWidth: (width: number) => void;
  resetWidth: () => void;
  query: string;
  onQueryChange: (q: string) => void;
  queue: ModelFile[];
  onQueueReorder: (orderedIds: string[]) => void;
  onQueueRemove: (id: string) => void;
  onQueueSelect: (id: string) => void;
  queueFilament?: Map<string, FilamentCheck> | null;
  folders: Folder[];
  totalModelCount: number;
  onCatalogRemoved?: () => void;
  activeFolderId: string;
  onFolderSelect: (id: string) => void;
  onCreateFolder: (parentId: string | null, name: string) => void;
  draggedFileId?: string | null;
  draggedFileFolderId?: string | null;
  dragOverFolderId?: string | null;
  draggedFolderId?: string | null;
  onFolderMouseEnter?: (id: string) => void;
  onFolderMouseLeave?: (id: string) => void;
  onDragFolderStart?: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  onTagSelect: (label: string | null) => void;
  collections: Collection[];
  activeCollection: string | null;
  collectionsGalleryOpen: boolean;
  onSelectCollection: (id: string) => void;
  onOpenCollectionsGallery: () => void;
  onCreateCollection: (name: string) => void;
  onRenameCollection: (id: string, name: string) => void | Promise<void>;
  onDeleteCollection: (id: string) => void | Promise<void>;
  toolView: ToolView | null;
  onToolViewChange: (v: ToolView | null) => void;
  toolCounts: ToolCounts;
  onOpenCleanup: () => void;
  cleanupScanning: boolean;
  cleanupError: AppError | null;
}

export function Sidebar({
  catalogKey,
  expansion,
  allFoldersCollapsed,
  onToggleAllFolders,
  width,
  setWidth,
  resetWidth,
  query,
  onQueryChange,
  queue,
  onQueueReorder,
  onQueueRemove,
  onQueueSelect,
  queueFilament,
  folders,
  totalModelCount,
  onCatalogRemoved,
  activeFolderId,
  onFolderSelect,
  onCreateFolder,
  draggedFileId = null,
  draggedFileFolderId = null,
  dragOverFolderId = null,
  draggedFolderId = null,
  onFolderMouseEnter,
  onFolderMouseLeave,
  onDragFolderStart,
  tags,
  activeTag,
  onTagSelect,
  collections,
  activeCollection,
  collectionsGalleryOpen,
  onSelectCollection,
  onOpenCollectionsGallery,
  onCreateCollection,
  onRenameCollection,
  onDeleteCollection,
  toolView,
  onToolViewChange,
  toolCounts,
  onOpenCleanup,
  cleanupScanning,
  cleanupError,
}: Props) {
  const { jobActive, lockProps } = useImportLock();
  const [collectionMenu, setCollectionMenu] = useState<{ collection: Collection; x: number; y: number } | null>(null);
  const [deletingCollection, setDeletingCollection] = useState<Collection | null>(null);
  const [renamingCollection, setRenamingCollection] = useState<Collection | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  const [collectionError, setCollectionError] = useState<string | null>(null);
  const [collectionBusy, setCollectionBusy] = useState(false);
  const collectionTrigger = useRef<HTMLElement | null>(null);
  const collectionsHeading = useRef<HTMLSpanElement>(null);
  const renameInput = useRef<HTMLInputElement>(null);
  useEffect(() => { if (renamingCollection) renameInput.current?.focus(); }, [renamingCollection]);
  const closeCollectionDialog = () => { if (!collectionBusy) { setDeletingCollection(null); setCollectionError(null); } };
  const submitCollectionRename = async () => {
    if (jobActive || collectionBusy || !renamingCollection) return;
    const name = renameDraft.trim();
    if (!name || name === renamingCollection.name) {
      setRenamingCollection(null); collectionTrigger.current?.focus(); return;
    }
    setCollectionBusy(true); setCollectionError(null);
    try { await onRenameCollection(renamingCollection.id, name); setRenamingCollection(null); collectionTrigger.current?.focus(); }
    catch (error) { setCollectionError(messageOf(error)); }
    finally { setCollectionBusy(false); }
  };

  const t = useT();
  const [dragging, setDragging] = useState(false);
  const stopDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => stopDrag.current?.(), []);
  const toggleLabel = t(allFoldersCollapsed ? 'expandAllFolders' : 'collapseAllFolders');
  const { language } = useLanguage();
  const [tagQuery, setTagQuery] = useState('');
  useEffect(() => setTagQuery(''), [catalogKey]);
  const [showSingleTags, setShowSingleTags] = useState(false);
  const hiddenSingleCount = tags.filter(tag => tag.count === 1 && tag.label !== activeTag).length;
  const visibleTags = sortTagsForDisplay(tags, language).filter(tag => tagQuery.trim()
    ? tagMatches(tag.label, tagQuery.trim(), language)
    : tag.count >= 2 || tag.label === activeTag || (showSingleTags && tag.count === 1));
  const [tagsCollapsed, setTagsCollapsed] = useState(true);
  const [creatingFolder, setCreatingFolder] = useState(false);
  const [folderNameDraft, setFolderNameDraft] = useState('');
  const [creatingCollection, setCreatingCollection] = useState(false);
  const [collectionNameDraft, setCollectionNameDraft] = useState('');

  const submitCreateCollection = () => {
    const value = collectionNameDraft.trim();
    if (value) onCreateCollection(value);
    setCollectionNameDraft('');
    setCreatingCollection(false);
  };

  // Inline input instead of window.prompt. The folder is created below the
  // active folder, at the root for "All models".
  const submitCreateFolder = () => {
    if (jobActive) return;
    const value = folderNameDraft.trim();
    if (value) onCreateFolder(activeFolderId === 'all' ? null : activeFolderId, value);
    setFolderNameDraft('');
    setCreatingFolder(false);
  };

  return (
    <aside style={{ width }} className="relative flex-none flex flex-col min-h-0 bg-[var(--panel)] border-r border-[var(--line)]">
      <div
        role="separator" aria-orientation="vertical" aria-label={t('sidebarResizeHandle')}
        aria-valuemin={SIDEBAR_MIN} aria-valuemax={SIDEBAR_MAX} aria-valuenow={width} tabIndex={0}
        className={`group absolute top-0 bottom-0 -right-1 w-2 z-30 cursor-col-resize outline-none ${dragging ? 'is-dragging' : ''}`}
        onDoubleClick={resetWidth}
        onKeyDown={(event) => {
          const next = { ArrowLeft: width - SIDEBAR_STEP, ArrowRight: width + SIDEBAR_STEP,
            Home: SIDEBAR_MIN, End: SIDEBAR_MAX }[event.key];
          if (next === undefined) return;
          event.preventDefault();
          event.stopPropagation();
          setWidth(next);
        }}
        onMouseDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          event.currentTarget.focus();
          stopDrag.current?.();
          const startX = event.clientX;
          const startWidth = width;
          const previousSelect = document.body.style.userSelect;
          const move = (moveEvent: MouseEvent) => setWidth(startWidth + moveEvent.clientX - startX);
          const stop = () => {
            window.removeEventListener('mousemove', move);
            window.removeEventListener('mouseup', stop);
            window.removeEventListener('blur', stop);
            document.body.style.userSelect = previousSelect;
            stopDrag.current = null;
            setDragging(false);
          };
          stopDrag.current = stop;
          document.body.style.userSelect = 'none';
          setDragging(true);
          window.addEventListener('mousemove', move);
          window.addEventListener('mouseup', stop);
          window.addEventListener('blur', stop);
        }}
      >
        <span className="absolute inset-y-0 left-[3px] w-0.5 bg-[var(--accent)] opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100 group-[.is-dragging]:opacity-100" />
      </div>
      <div className="p-3 pb-2.5 border-b border-[var(--line)]">
        <div className="flex items-center gap-1.5 h-8 px-2.5 rounded-[3px] border border-[var(--line)] focus-within:border-[var(--accent)] bg-[var(--panel-2)]">
          <span className="font-medium tabular-nums text-small text-[var(--ink-3)]"><Icon name="search" size={14} /></span>
          <input
            id={SEARCH_INPUT_ID}
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder={t('searchPlaceholder')}
            className="flex-1 min-w-0 border-0 outline-0 bg-transparent text-[var(--ink)] text-[length:var(--font-size-body)]"
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2 py-3">
        <div className="group relative flex items-center gap-1 px-1.5 pb-2">
          <span className="inline-flex items-center gap-1.5 ui-label text-[var(--ink-3)]">
            <Icon name="folder" size={16} />
            {t('foldersHeading')}
          </span>
          <span className="grid place-items-center text-[var(--ink-3)] cursor-default">
            <Icon name="info" size={16} />
          </span>
          <span className="pointer-events-none absolute top-[20px] right-0 z-20 w-[200px] rounded-[8px] bg-[var(--ink)] px-2.5 py-2 text-caption font-sans font-medium leading-[1.4] text-[var(--bg)] opacity-0 -translate-y-0.5 transition-opacity transition-transform group-hover:opacity-100 group-hover:translate-y-0">
            {t('foldersInfoTooltip')}
          </span>
          <button type="button" aria-label={toggleLabel} title={toggleLabel} onClick={onToggleAllFolders}
            className="ml-auto shrink-0 w-[22px] h-[22px] grid place-items-center rounded-[3px] text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
            <Icon name={allFoldersCollapsed ? 'expand-all' : 'collapse-all'} size={16} />
          </button>
        </div>
        <FolderTree
          expansion={expansion}
          folders={folders}
          totalModelCount={totalModelCount}
          activeFolderId={activeFolderId}
          onSelect={onFolderSelect}
          onRemoved={onCatalogRemoved}
          draggedFileId={draggedFileId}
          draggedFileFolderId={draggedFileFolderId}
          dragOverFolderId={dragOverFolderId}
          draggedFolderId={draggedFolderId}
          onFolderMouseEnter={onFolderMouseEnter}
          onFolderMouseLeave={onFolderMouseLeave}
          onDragFolderStart={jobActive ? undefined : onDragFolderStart}
        />
        {creatingFolder ? (
          <div className="flex items-center gap-1.5 px-1.5 pt-1 pb-1">
            <input
              value={folderNameDraft}
              {...lockProps}
              onChange={(e) => setFolderNameDraft(e.target.value)}
              onBlur={submitCreateFolder}
              onKeyDown={(e) => {
                if (e.key === 'Enter') submitCreateFolder();
                if (e.key === 'Escape') {
                  setCreatingFolder(false);
                  setFolderNameDraft('');
                }
              }}
              autoFocus
              placeholder={t('newFolderPlaceholder')}
              className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-caption"
            />
          </div>
        ) : (
          <button type="button" {...lockProps}
            onClick={() => {
              setCreatingFolder(true);
              setFolderNameDraft('');
            }}
            className="flex items-center h-7 px-1.5 rounded-[3px] border border-dashed border-[var(--line-strong)] cursor-pointer font-medium tabular-nums text-caption text-[var(--ink-3)] hover:border-[var(--accent)] hover:text-[var(--accent)]"
           aria-label={t('createFolderLabel')}>
            <Icon name="plus" size={14} /> {t('createFolderLabel').replace(/^\+\s*/, '')}
          </button>
        )}

        <div className="flex items-center justify-between px-1.5 pt-[18px] pb-2">
          <span ref={collectionsHeading} tabIndex={-1} className="inline-flex items-center gap-1.5 ui-label text-[var(--ink-3)]">
            <Icon name="layers" size={16} />
            {t('collectionsTab')}
          </span>
          <span
            onClick={onOpenCollectionsGallery}
            className="font-medium tabular-nums text-caption text-[var(--accent)] cursor-pointer hover:underline"
          >
            {t('viewAllCollectionsLabel')}
          </span>
        </div>
        {collections.map((c) => (
          <div
            key={c.id}
            role="button"
            tabIndex={0}
            aria-label={c.name}
            onClick={() => { if (renamingCollection?.id !== c.id) onSelectCollection(c.id); }}
            onContextMenu={(event) => {
              event.preventDefault();
              if (renamingCollection) return;
              event.currentTarget.focus(); collectionTrigger.current = event.currentTarget;
              setCollectionMenu({ collection: c, x: event.clientX, y: event.clientY });
            }}
            onKeyDown={(event) => {
              if (event.target !== event.currentTarget) return;
              if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
                event.preventDefault(); event.currentTarget.focus(); collectionTrigger.current = event.currentTarget;
                const bounds = event.currentTarget.getBoundingClientRect();
                setCollectionMenu({ collection: c, x: bounds.left, y: bounds.bottom });
              } else if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); onSelectCollection(c.id); }
            }}
            className={`flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-pointer text-[length:var(--font-size-item)] ${
              !collectionsGalleryOpen && activeCollection === c.id
                ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold hover:text-[var(--ink)]'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            {renamingCollection?.id === c.id ? <input ref={renameInput} aria-label={t('renameCollectionAria')}
              value={renameDraft} onChange={event => setRenameDraft(event.target.value)} disabled={collectionBusy || jobActive}
              onClick={event => event.stopPropagation()}
              onKeyDown={event => {
                if (event.key === 'Enter') { event.preventDefault(); void submitCollectionRename(); }
                if (event.key === 'Escape') { event.stopPropagation(); if (!collectionBusy) { setRenamingCollection(null); setCollectionError(null); collectionTrigger.current?.focus(); } }
              }} className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] text-caption" />
              : <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{c.name}</span>}
            <span className="font-medium tabular-nums text-caption text-[var(--ink-3)]">{c.modelCount}</span>
          </div>
        ))}
        {creatingCollection ? (
          <div className="flex items-center gap-1.5 px-1.5 pt-1 pb-1">
            <input
              value={collectionNameDraft}
              onChange={(e) => setCollectionNameDraft(e.target.value)}
              onBlur={submitCreateCollection}
              onKeyDown={(e) => {
                if (e.key === 'Enter') submitCreateCollection();
                if (e.key === 'Escape') {
                  setCreatingCollection(false);
                  setCollectionNameDraft('');
                }
              }}
              autoFocus
              placeholder={t('newCollectionPlaceholder')}
              className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-caption"
            />
          </div>
        ) : (
          <div
            onClick={() => {
              setCreatingCollection(true);
              setCollectionNameDraft('');
            }}
            className="flex items-center h-7 px-1.5 rounded-[3px] border border-dashed border-[var(--line-strong)] cursor-pointer font-medium tabular-nums text-caption text-[var(--ink-3)] hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('createCollectionLabel')}
          </div>
        )}

        <div
          onClick={() => setTagsCollapsed((c) => !c)}
          className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer"
        >
          <span className="inline-flex items-center gap-1.5 ui-label text-[var(--ink-3)]">
            <Icon name="tag" size={16} />
            {t('tagsHeading')}
          </span>
          <span className="font-medium tabular-nums text-[length:var(--font-size-label)] leading-none text-[var(--ink-3)]">
            <Icon name="chevron" size={14} className={tagsCollapsed  ? '' : 'rotate-180'} />
          </span>
        </div>
        {!tagsCollapsed && (
          <div className="px-1.5 pb-1">
            <div className="flex items-center h-[26px] mb-2 rounded border border-[var(--line)] bg-[var(--panel-2)] focus-within:border-[var(--accent)]">
              <input value={tagQuery} onChange={event => setTagQuery(event.target.value)}
                placeholder={t('tagSearchPlaceholder')} aria-label={t('tagSearchPlaceholder')}
                className="w-full min-w-0 bg-transparent px-2 text-small outline-none"
                onKeyDown={event => {
                  if (event.key === 'Escape') { event.stopPropagation(); setTagQuery(''); }
                  if (event.key === 'Enter' && visibleTags[0]) { event.preventDefault(); onTagSelect(visibleTags[0].label); }
                }} />
              {tagQuery && <button aria-label={`${t('tagSearchPlaceholder')} ${t('delete')}`} onClick={() => setTagQuery('')}
                className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] px-1 text-[var(--ink-3)]"><Icon name="close" size={14} /></button>}
            </div>
            {visibleTags.length === 0 && tagQuery.trim() && <p className="text-small text-[var(--ink-3)]">{t('tagSearchEmpty')}</p>}
            <div className="flex flex-wrap gap-1.5">
            {visibleTags.map((tag) => (
              <button
                key={tag.label}
                onClick={() => onTagSelect(activeTag === tag.label ? null : tag.label)}
                className={`inline-flex items-center gap-1.5 h-6 px-2 rounded-full border cursor-pointer font-medium tabular-nums text-caption ${
                  activeTag === tag.label
                    ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] hover:text-[var(--ink)]'
                    : 'border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] hover:text-[var(--ink)]'
                }`}
              >
                <TagDot hue={tag.colorHue} />
                #{tagLabel(tag.label, language)}
                <span className="text-[var(--ink-3)]">{tag.count}</span>
              </button>
            ))}
            </div>
            {hiddenSingleCount > 0 && !tagQuery.trim() && (
              <button type="button" aria-expanded={showSingleTags} onClick={() => setShowSingleTags(show => !show)}
                className="mt-2 text-left font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--ink-2)] hover:text-[var(--ink)] cursor-pointer">
                {showSingleTags ? t('tagsHideSingles') : t('tagsShowSingles').replace('{count}', String(hiddenSingleCount))}
              </button>
            )}
          </div>
        )}

        <ToolsSection
          queue={queue}
          onQueueReorder={onQueueReorder}
          onQueueRemove={onQueueRemove}
          onQueueSelect={onQueueSelect}
          queueFilament={queueFilament}
          toolView={toolView}
          onToolViewChange={onToolViewChange}
          counts={toolCounts}
          onOpenCleanup={onOpenCleanup}
          cleanupScanning={cleanupScanning}
          cleanupError={cleanupError}
        />
      </div>
      {collectionError && !deletingCollection && <p role="alert" className="px-3 text-[var(--crit)]">{collectionError}</p>}
      {collectionMenu && <ContextMenu x={collectionMenu.x} y={collectionMenu.y} onClose={() => setCollectionMenu(null)}
        collectionActions={{
          onRename: () => { if (jobActive) return; setCollectionError(null); setRenameDraft(collectionMenu.collection.name); setRenamingCollection(collectionMenu.collection); setCollectionMenu(null); },
          onDelete: () => { if (jobActive) return; setCollectionError(null); setDeletingCollection(collectionMenu.collection); setCollectionMenu(null); },
        }} />}
      {deletingCollection && <CatalogActionDialog title={t('deleteCollectionConfirmQuestion')} returnFocus={collectionTrigger} onClose={closeCollectionDialog}>
        {collectionError && <p role="alert">{collectionError}</p>}
        <div className="flex justify-end gap-2">
          <button data-initial-focus className={catalogActionButton} disabled={collectionBusy} onClick={closeCollectionDialog}>{t('cancel')}</button>
          <button className={catalogActionButton} {...lockProps} disabled={jobActive || collectionBusy} onClick={async () => {
            if (jobActive || collectionBusy) return;
            setCollectionBusy(true); setCollectionError(null);
            try {
              await onDeleteCollection(deletingCollection.id);
              collectionTrigger.current = collectionsHeading.current;
              setDeletingCollection(null);
            } catch (error) { setCollectionError(messageOf(error)); }
            finally { setCollectionBusy(false); }
          }}>{t('delete')}</button>
        </div>
      </CatalogActionDialog>}
    </aside>
  );
}
