import { useEffect, useState } from 'react';
import type { Folder, TagCount, CreatorCount, ModelFile, SavedFilter } from '../types';
import { useT } from '../i18n/LanguageContext';
import { FolderTree } from './FolderTree';

interface Props {
  query: string;
  onQueryChange: (q: string) => void;
  queue: ModelFile[];
  onQueueReorder: (orderedIds: string[]) => void;
  onQueueRemove: (id: string) => void;
  onQueueSelect: (id: string) => void;
  folders: Folder[];
  activeFolderId: string;
  onFolderSelect: (id: string) => void;
  onCreateFolder: (parentId: string | null, name: string) => void;
  dragOverFolderId?: string | null;
  onFolderMouseEnter?: (id: string) => void;
  onDragFolderStart?: (id: string) => void;
  tags: TagCount[];
  activeTag: string | null;
  onTagSelect: (label: string | null) => void;
  creators: CreatorCount[];
  activeCreator: string | null;
  onCreatorSelect: (label: string | null) => void;
  savedFilters: SavedFilter[];
  onSaveFilter: (name: string) => void;
  onApplyFilter: (filter: SavedFilter) => void;
  onDeleteFilter: (id: string) => void;
}

export function Sidebar({
  query,
  onQueryChange,
  queue,
  onQueueReorder,
  onQueueRemove,
  onQueueSelect,
  folders,
  activeFolderId,
  onFolderSelect,
  onCreateFolder,
  dragOverFolderId = null,
  onFolderMouseEnter,
  onDragFolderStart,
  tags,
  activeTag,
  onTagSelect,
  creators,
  activeCreator,
  onCreatorSelect,
  savedFilters,
  onSaveFilter,
  onApplyFilter,
  onDeleteFilter,
}: Props) {
  const t = useT();
  const [tagsCollapsed, setTagsCollapsed] = useState(true);
  const [creatorsCollapsed, setCreatorsCollapsed] = useState(true);
  const [filtersCollapsed, setFiltersCollapsed] = useState(false);
  const [savingFilter, setSavingFilter] = useState(false);
  const [filterNameDraft, setFilterNameDraft] = useState('');
  const [queueCollapsed, setQueueCollapsed] = useState(false);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);
  const [creatingFolder, setCreatingFolder] = useState(false);
  const [folderNameDraft, setFolderNameDraft] = useState('');

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

  // Reihenfolge-Aenderung per Maus-Events statt nativem HTML5-Drag&Drop
  // (draggable/onDragStart/onDragOver/onDrop): Tauri faengt bei aktiviertem
  // dragDropEnabled (Standard, wird fuer den Datei-Import per OS-Drop
  // benoetigt) native Drag-Sessions auf Fenster-Ebene ab, was unter
  // WebKitGTK In-Page-HTML5-DnD zuverlaessig verhindert. Ein rein
  // JS-gesteuerter Mouse-Down/Enter/Up-Ablauf umgeht das native DnD-System
  // vollstaendig.
  useEffect(() => {
    if (dragIndex === null) return;
    const handleMouseUp = () => {
      const from = dragIndex;
      const to = overIndex;
      setDragIndex(null);
      setOverIndex(null);
      if (from === null) return;
      if (to === null || to === from) {
        onQueueSelect(queue[from].id);
        return;
      }
      const ids = queue.map((m) => m.id);
      const [moved] = ids.splice(from, 1);
      ids.splice(to, 0, moved);
      onQueueReorder(ids);
    };
    document.addEventListener('mouseup', handleMouseUp);
    return () => document.removeEventListener('mouseup', handleMouseUp);
  }, [dragIndex, overIndex, queue, onQueueReorder, onQueueSelect]);

  return (
    <aside className="flex-none w-[242px] flex flex-col min-h-0 bg-[var(--panel)] border-r border-[var(--line)]">
      <div className="p-3 pb-2.5 border-b border-[var(--line)]">
        <div className="flex items-center gap-1.5 h-8 px-2.5 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)]">
          <span className="font-mono-ui text-xs text-[var(--ink-3)]">⌕</span>
          <input
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder={t('searchPlaceholder')}
            className="flex-1 min-w-0 border-0 outline-0 bg-transparent text-[var(--ink)] text-[length:var(--font-size-body)]"
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2 py-3">
        <div className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)] px-1.5 pb-2">
          {t('foldersHeading')}
        </div>
        <FolderTree
          folders={folders}
          activeFolderId={activeFolderId}
          onSelect={onFolderSelect}
          dragOverFolderId={dragOverFolderId}
          onFolderMouseEnter={onFolderMouseEnter}
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
            className="flex items-center h-7 px-1.5 rounded-[3px] cursor-pointer font-mono-ui text-[11.5px] text-[var(--ink-3)] hover:text-[var(--accent)]"
          >
            {t('createFolderLabel')}
          </div>
        )}

        <div
          onClick={() => setQueueCollapsed((c) => !c)}
          className="flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer"
        >
          <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('queueHeading')}
          </span>
          <span className="font-mono-ui text-[length:var(--font-size-label)] leading-none text-[var(--ink-3)]">
            {queueCollapsed ? '▾' : '▴'}
          </span>
        </div>
        {!queueCollapsed && queue.length === 0 && (
          <div className="px-1.5 pb-2 font-mono-ui text-[10.5px] text-[var(--ink-3)]">
            {t('queueEmptyState')}
          </div>
        )}
        {!queueCollapsed && queue.map((model, index) => (
          <div
            key={model.id}
            onMouseDown={() => {
              setDragIndex(index);
              setOverIndex(index);
            }}
            onMouseEnter={() => {
              if (dragIndex !== null) setOverIndex(index);
            }}
            className={`flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-grab select-none ${
              dragIndex === index ? 'opacity-50' : ''
            } ${
              dragIndex !== null && overIndex === index && dragIndex !== index
                ? 'bg-[var(--panel-2)]'
                : ''
            } text-[var(--ink-2)] hover:text-[var(--ink)]`}
          >
            <span className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)] w-3.5">{index + 1}</span>
            <span className="flex-1 text-[length:var(--font-size-item)] overflow-hidden text-ellipsis whitespace-nowrap">
              {model.name}
            </span>
            <span
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                onQueueRemove(model.id);
              }}
              className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
            >
              ✕
            </span>
          </div>
        ))}

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
            {tags
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
                #{tag.label}
                <span className="text-[var(--ink-3)]">{tag.count}</span>
              </span>
            ))}
          </div>
        )}

        <div
          onClick={() => setCreatorsCollapsed((c) => !c)}
          className="flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer"
        >
          <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('creatorsHeading')}
          </span>
          <span className="font-mono-ui text-[length:var(--font-size-label)] leading-none text-[var(--ink-3)]">
            {creatorsCollapsed ? '▾' : '▴'}
          </span>
        </div>
        {!creatorsCollapsed && (
          <div className="flex flex-wrap gap-1.5 px-1.5 pb-1">
            {creators.map((creator) => (
              <span
                key={creator.label}
                onClick={() => onCreatorSelect(activeCreator === creator.label ? null : creator.label)}
                className={`inline-flex items-center gap-1.5 h-6 px-2 rounded-full border cursor-pointer font-mono-ui text-[11.5px] ${
                  activeCreator === creator.label
                    ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                    : 'border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] hover:text-[var(--ink)]'
                }`}
              >
                {creator.label}
                <span className="text-[var(--ink-3)]">{creator.count}</span>
              </span>
            ))}
          </div>
        )}

        <div className="flex items-center justify-between px-1.5 pt-[18px] pb-2">
          <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('savedFiltersHeading')}
          </span>
          <div className="flex items-center gap-2">
            <span
              onClick={() => {
                setFiltersCollapsed(false);
                setSavingFilter(true);
                setFilterNameDraft('');
              }}
              className="font-mono-ui text-sm leading-none text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
            >
              +
            </span>
            <span
              onClick={() => setFiltersCollapsed((c) => !c)}
              className="font-mono-ui text-[length:var(--font-size-label)] leading-none text-[var(--ink-3)] cursor-pointer"
            >
              {filtersCollapsed ? '▾' : '▴'}
            </span>
          </div>
        </div>
        {!filtersCollapsed && savingFilter && (
          <div className="flex items-center gap-1.5 px-1.5 pb-2">
            <input
              value={filterNameDraft}
              onChange={(e) => setFilterNameDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  const name = filterNameDraft.trim();
                  if (name) onSaveFilter(name);
                  setSavingFilter(false);
                }
                if (e.key === 'Escape') setSavingFilter(false);
              }}
              autoFocus
              placeholder={t('savedFilterNamePlaceholder')}
              className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[11.5px]"
            />
            <span
              onClick={() => {
                const name = filterNameDraft.trim();
                if (name) onSaveFilter(name);
                setSavingFilter(false);
              }}
              className="font-mono-ui text-[11px] text-[var(--accent)] cursor-pointer"
            >
              ✓
            </span>
          </div>
        )}
        {!filtersCollapsed && savedFilters.map((filter) => (
          <div
            key={filter.id}
            onClick={() => onApplyFilter(filter)}
            className="flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-pointer text-[var(--ink-2)] hover:text-[var(--ink)]"
          >
            <span className="flex-1 text-[length:var(--font-size-item)] overflow-hidden text-ellipsis whitespace-nowrap">
              {filter.name}
            </span>
            <span
              onClick={(e) => {
                e.stopPropagation();
                onDeleteFilter(filter.id);
              }}
              className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
            >
              ✕
            </span>
          </div>
        ))}
      </div>
    </aside>
  );
}
