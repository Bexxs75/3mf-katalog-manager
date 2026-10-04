import type { useFolderExpansion } from '../hooks/useFolderExpansion';
import { SIDEBAR_MIN, SIDEBAR_MAX, SIDEBAR_STEP } from '../hooks/useSidebarWidth';
import { Icon } from './Icon';
import { useEffect, useRef, useState } from 'react';
import type { Folder, TagCount, ModelFile, Collection, FilamentCheck } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { SEARCH_INPUT_ID } from '../hooks/useKeyboardShortcuts';
import { FolderTree } from './FolderTree';
import { sortTagsForDisplay, tagLabel } from '../lib/autoTags';
import { ToolsSection } from './ToolsSection';
import type { ToolCounts, ToolView } from '../lib/toolViews';
import type { AppError } from '../lib/errors';

interface Props {
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
  toolView: ToolView | null;
  onToolViewChange: (v: ToolView | null) => void;
  toolCounts: ToolCounts;
  onOpenCleanup: () => void;
  cleanupScanning: boolean;
  cleanupError: AppError | null;
}

export function Sidebar({
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
  toolView,
  onToolViewChange,
  toolCounts,
  onOpenCleanup,
  cleanupScanning,
  cleanupError,
}: Props) {
  const t = useT();
  const [dragging, setDragging] = useState(false);
  const stopDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => stopDrag.current?.(), []);
  const toggleLabel = t(allFoldersCollapsed ? 'expandAllFolders' : 'collapseAllFolders');
  const { language } = useLanguage();
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
          <span className="font-mono-ui text-xs text-[var(--ink-3)]">⌕</span>
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
          <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('foldersHeading')}
          </span>
          <span className="w-[13px] h-[13px] rounded-full border border-[var(--ink-3)] grid place-items-center font-mono-ui text-[9px] text-[var(--ink-3)] cursor-default">
            i
          </span>
          <span className="pointer-events-none absolute top-[20px] right-0 z-20 w-[200px] rounded-[8px] bg-[var(--ink)] px-2.5 py-2 text-[11px] font-sans font-medium leading-[1.4] text-[var(--bg)] opacity-0 -translate-y-0.5 transition-opacity transition-transform group-hover:opacity-100 group-hover:translate-y-0">
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
          onDragFolderStart={onDragFolderStart}
        />
        {creatingFolder ? (
          <div className="flex items-center gap-1.5 px-1.5 pt-1 pb-1">
            <input
              value={folderNameDraft}
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
              className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[11.5px]"
            />
          </div>
        ) : (
          <div
            onClick={() => {
              setCreatingFolder(true);
              setFolderNameDraft('');
            }}
            className="flex items-center h-7 px-1.5 rounded-[3px] border border-dashed border-[var(--line-strong)] cursor-pointer font-mono-ui text-[11.5px] text-[var(--ink-3)] hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('createFolderLabel')}
          </div>
        )}

        <div className="flex items-center justify-between px-1.5 pt-[18px] pb-2">
          <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('collectionsTab')}
          </span>
          <span
            onClick={onOpenCollectionsGallery}
            className="font-mono-ui text-[10.5px] text-[var(--accent)] cursor-pointer hover:underline"
          >
            {t('viewAllCollectionsLabel')}
          </span>
        </div>
        {collections.map((c) => (
          <div
            key={c.id}
            onClick={() => onSelectCollection(c.id)}
            className={`flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-pointer text-[length:var(--font-size-item)] ${
              !collectionsGalleryOpen && activeCollection === c.id
                ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{c.name}</span>
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">{c.modelCount}</span>
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
              className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[11.5px]"
            />
          </div>
        ) : (
          <div
            onClick={() => {
              setCreatingCollection(true);
              setCollectionNameDraft('');
            }}
            className="flex items-center h-7 px-1.5 rounded-[3px] border border-dashed border-[var(--line-strong)] cursor-pointer font-mono-ui text-[11.5px] text-[var(--ink-3)] hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('createCollectionLabel')}
          </div>
        )}

        <div
          onClick={() => setTagsCollapsed((c) => !c)}
          className="flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer"
        >
          <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('tagsHeading')}
          </span>
          <span className="font-mono-ui text-[length:var(--font-size-label)] leading-none text-[var(--ink-3)]">
            {tagsCollapsed ? '▾' : '▴'}
          </span>
        </div>
        {!tagsCollapsed && (
          <div className="flex flex-wrap gap-1.5 px-1.5 pb-1">
            {sortTagsForDisplay(tags, language)
              .filter((tag) => tag.count >= 2 || tag.label === activeTag)
              .map((tag) => (
              <span
                key={tag.label}
                onClick={() => onTagSelect(activeTag === tag.label ? null : tag.label)}
                className={`inline-flex items-center gap-1.5 h-6 px-2 rounded-full border cursor-pointer font-mono-ui text-[11.5px] ${
                  activeTag === tag.label
                    ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                    : 'border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] hover:text-[var(--ink)]'
                }`}
              >
                <span
                  className="w-[6px] h-[6px] rounded-full flex-none"
                  style={{ background: `oklch(0.62 0.14 ${tag.colorHue})` }}
                />
                #{tagLabel(tag.label, language)}
                <span className="text-[var(--ink-3)]">{tag.count}</span>
              </span>
            ))}
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
    </aside>
  );
}
