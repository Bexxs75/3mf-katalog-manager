import { useEffect, useState } from 'react';
import type { Folder, TagCount, CreatorCount, CloudAccount, ModelFile, SavedFilter } from '../types';
import { useT } from '../i18n/LanguageContext';

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
  clouds: CloudAccount[];
  cloudError: string | null;
  onAddCloud: () => void;
  onConnectCloud: (id: string) => void;
  onDisconnectCloud: (id: string) => void;
}

const originAbbr: Record<string, string> = {
  gdrive: 'GD',
  onedrive: 'OD',
  dropbox: 'DB',
  proton: 'PD',
};

// Anbieternamen bleiben immer im Original (Markenname), unabhängig von der
// aktiven UI-Sprache — nicht über t() übersetzt, analog zu den
// Sprachnamen im Sprache-Umschalter.
const providerName: Record<string, string> = {
  gdrive: 'Google Drive',
  onedrive: 'OneDrive',
  dropbox: 'Dropbox',
  proton: 'Proton Drive',
};

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
  clouds,
  cloudError,
  onAddCloud,
  onConnectCloud,
  onDisconnectCloud,
}: Props) {
  const t = useT();
  const [tagsCollapsed, setTagsCollapsed] = useState(false);
  const [creatorsCollapsed, setCreatorsCollapsed] = useState(false);
  const [filtersCollapsed, setFiltersCollapsed] = useState(false);
  const [savingFilter, setSavingFilter] = useState(false);
  const [filterNameDraft, setFilterNameDraft] = useState('');
  const [queueCollapsed, setQueueCollapsed] = useState(false);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);

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
            className="flex-1 min-w-0 border-0 outline-0 bg-transparent text-[var(--ink)] text-[var(--font-size-body)]"
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2 py-3">
        <div className="font-mono-ui text-[var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)] px-1.5 pb-2">
          {t('foldersHeading')}
        </div>
        {folders.map((f) => (
          <div
            key={f.id}
            onClick={() => onFolderSelect(f.id)}
            className={`flex items-center gap-2 h-8 px-1.5 rounded-[3px] text-[var(--font-size-body)] cursor-pointer ${
              f.id === activeFolderId
                ? 'bg-[var(--panel-2)] text-[var(--ink)]'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
              {f.name}
            </span>
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">{f.count}</span>
          </div>
        ))}

        <div
          onClick={() => setQueueCollapsed((c) => !c)}
          className="flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer"
        >
          <span className="font-mono-ui text-[var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('queueHeading')}
          </span>
          <span className="font-mono-ui text-[var(--font-size-label)] leading-none text-[var(--ink-3)]">
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
            <span className="font-mono-ui text-[var(--font-size-meta)] text-[var(--ink-3)] w-3.5">{index + 1}</span>
            <span className="flex-1 text-xs overflow-hidden text-ellipsis whitespace-nowrap">
              {model.name}
            </span>
            <span
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                onQueueRemove(model.id);
              }}
              className="font-mono-ui text-[var(--font-size-meta)] text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
            >
              ✕
            </span>
          </div>
        ))}

        <div
          onClick={() => setTagsCollapsed((c) => !c)}
          className="flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer"
        >
          <span className="font-mono-ui text-[var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('tagsHeading')}
          </span>
          <span className="font-mono-ui text-[var(--font-size-label)] leading-none text-[var(--ink-3)]">
            {tagsCollapsed ? '▾' : '▴'}
          </span>
        </div>
        {!tagsCollapsed && tags.map((tag) => (
          <div
            key={tag.label}
            onClick={() => onTagSelect(activeTag === tag.label ? null : tag.label)}
            className={`flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-pointer ${
              activeTag === tag.label
                ? 'bg-[var(--accent-soft)] text-[var(--accent)]'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            <span
              className="w-[7px] h-[7px] rounded-full"
              style={{ background: `oklch(0.62 0.14 ${tag.colorHue})` }}
            />
            <span className="flex-1 font-mono-ui text-xs overflow-hidden text-ellipsis whitespace-nowrap">
              #{tag.label}
            </span>
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">{tag.count}</span>
          </div>
        ))}

        <div
          onClick={() => setCreatorsCollapsed((c) => !c)}
          className="flex items-center justify-between px-1.5 pt-[18px] pb-2 cursor-pointer"
        >
          <span className="font-mono-ui text-[var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('creatorsHeading')}
          </span>
          <span className="font-mono-ui text-[var(--font-size-label)] leading-none text-[var(--ink-3)]">
            {creatorsCollapsed ? '▾' : '▴'}
          </span>
        </div>
        {!creatorsCollapsed && creators.map((creator) => (
          <div
            key={creator.label}
            onClick={() => onCreatorSelect(activeCreator === creator.label ? null : creator.label)}
            className={`flex items-center gap-2 h-7 px-1.5 rounded-[3px] cursor-pointer ${
              activeCreator === creator.label
                ? 'bg-[var(--accent-soft)] text-[var(--accent)]'
                : 'text-[var(--ink-2)] hover:text-[var(--ink)]'
            }`}
          >
            <span className="flex-1 font-mono-ui text-xs overflow-hidden text-ellipsis whitespace-nowrap">
              {creator.label}
            </span>
            <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">{creator.count}</span>
          </div>
        ))}

        <div className="flex items-center justify-between px-1.5 pt-[18px] pb-2">
          <span className="font-mono-ui text-[var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
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
              className="font-mono-ui text-[var(--font-size-label)] leading-none text-[var(--ink-3)] cursor-pointer"
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
            <span className="flex-1 text-xs overflow-hidden text-ellipsis whitespace-nowrap">
              {filter.name}
            </span>
            <span
              onClick={(e) => {
                e.stopPropagation();
                onDeleteFilter(filter.id);
              }}
              className="font-mono-ui text-[var(--font-size-meta)] text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
            >
              ✕
            </span>
          </div>
        ))}
      </div>

      <div className="flex-none border-t border-[var(--line)] px-3.5 pt-3 pb-3.5">
        <div className="flex items-center justify-between pb-2.5">
          <span className="font-mono-ui text-[var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)]">
            {t('cloudAccountsHeading')}
          </span>
          <span
            onClick={onAddCloud}
            className="font-mono-ui text-sm leading-none text-[var(--ink-3)] cursor-pointer hover:text-[var(--accent)]"
          >
            +
          </span>
        </div>
        {clouds.map((c) => (
          <div key={c.id} className="flex flex-col gap-1.5 py-1.5">
            <div className="flex items-center gap-2">
              <span className="font-mono-ui text-[var(--font-size-meta)] px-1 py-0.5 rounded border border-[var(--line-strong)] text-[var(--ink-2)]">
                {originAbbr[c.id] ?? c.abbr}
              </span>
              <span
                className="flex-1 text-[var(--font-size-title)] font-medium overflow-hidden text-ellipsis whitespace-nowrap"
                title={c.name}
              >
                {providerName[c.id] ?? c.name}
              </span>
              <span
                onClick={() =>
                  c.status === 'disconnected' ? onConnectCloud(c.id) : onDisconnectCloud(c.id)
                }
                className={`font-mono-ui text-[var(--font-size-meta)] cursor-pointer ${
                  c.status === 'connected'
                    ? 'text-[var(--ink-3)] hover:text-[var(--accent)]'
                    : 'text-[var(--accent)] hover:opacity-70'
                }`}
              >
                {c.status === 'connected'
                  ? t('cloudConnected')
                  : c.status === 'error'
                  ? t('cloudError')
                  : t('cloudDisconnected')}
              </span>
            </div>
            <div className="flex items-center gap-2 pl-[26px]">
              <div className="flex-1 h-[3px] rounded bg-[var(--line)] overflow-hidden">
                <div
                  className="h-full bg-[var(--accent)]"
                  style={{ width: `${c.usedPercent}%` }}
                />
              </div>
              <span className="font-mono-ui text-[var(--font-size-meta)] text-[var(--ink-3)] whitespace-nowrap">
                {c.quotaLabel}
              </span>
            </div>
          </div>
        ))}
        {cloudError && (
          <div className="pt-1.5 font-mono-ui text-[var(--font-size-meta)] text-[var(--accent)] break-words">
            {t('cloudConnectionError')} {cloudError}
          </div>
        )}
      </div>
    </aside>
  );
}
