import { useState } from 'react';
import type { Folder, TagCount, ModelFile, Collection, FilamentCheck } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { SEARCH_INPUT_ID } from '../hooks/useKeyboardShortcuts';
import { FolderTree } from './FolderTree';
import { sortTagsForDisplay, tagLabel } from '../lib/autoTags';
import { ToolsSection } from './ToolsSection';
import type { ToolCounts, ToolView } from '../lib/toolViews';

interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  queue: ModelFile[];
  onQueueReorder: (orderedIds: string[]) => void;
  onQueueRemove: (id: string) => void;
  onQueueSelect: (id: string) => void;
  queueFilament?: Map<string, FilamentCheck> | null;
  folders: Folder[];
  totalModelCount: number;
  activeFolderId: string;
  onFolderSelect: (id: string) => void;
  onCreateFolder: (parentId: string | null, name: string) => void;
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
  trashCount: number;
  onOpenCleanup: () => void;
  cleanupScanning: boolean;
  cleanupError: string | null;
  onOpenFilament: () => void;
  onOpenTrash: () => void;
}

export function Sidebar({
  query,
  onQueryChange,
  queue,
  onQueueReorder,
  onQueueRemove,
  onQueueSelect,
  queueFilament,
  folders,
  totalModelCount,
  activeFolderId,
  onFolderSelect,
  onCreateFolder,
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
  trashCount,
  onOpenCleanup,
  cleanupScanning,
  cleanupError,
  onOpenFilament,
  onOpenTrash,
}: Props) {
  const t = useT();
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

  // Inline-Input statt window.prompt, analog zum `creating`-Muster in
  // CollectionsGallery.tsx (autoFocus + onBlur + Enter-Submit). Der neue
  // Ordner wird unter dem gerade aktiven Ordner angelegt (bzw. an der Wurzel,
  // wenn "Alle Modelle" aktiv ist - `onCreateFolder` in App.tsx macht daraus
  // `parentId: null`).
  const submitCreateFolder = () => {
    const value = folderNameDraft.trim();
    if (value) onCreateFolder(activeFolderId === 'all' ? null : activeFolderId, value);
    setFolderNameDraft('');
    setCreatingFolder(false);
  };

  return (
    <aside className="flex-none w-[242px] flex flex-col min-h-0 bg-[var(--panel)] border-r border-[var(--line)]">
      <div className="p-3 pb-2.5 border-b border-[var(--line)]">
        <div className="flex items-center gap-1.5 h-8 px-2.5 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)]">
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
        </div>
        <FolderTree
          folders={folders}
          totalModelCount={totalModelCount}
          activeFolderId={activeFolderId}
          onSelect={onFolderSelect}
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
          trashCount={trashCount}
          onOpenCleanup={onOpenCleanup}
          cleanupScanning={cleanupScanning}
          cleanupError={cleanupError}
          onOpenFilament={onOpenFilament}
          onOpenTrash={onOpenTrash}
        />
      </div>
    </aside>
  );
}
