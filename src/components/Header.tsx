import { useMenuKeyboard } from '../hooks/useMenuKeyboard';
import { Icon } from './Icon';
import { useEffect, useRef, useState } from 'react';
import { useDismissableMenu } from '../hooks/useDismissableMenu';
import type { MainView, ViewMode, SortKey, SortDirection } from '../types';
import { defaultSortDirection } from '../lib/catalogFilters';
import { formatCount } from '../i18n/types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  importTargetName?: string;
  importTargetIsRoot?: boolean;
  allFoldersCollapsed: boolean;
  onToggleAllFolders: () => void;
  view: ViewMode;
  onViewChange: (v: ViewMode) => void;
  sort: SortKey;
  sortDirection: SortDirection;
  onSortChange: (s: SortKey, direction: SortDirection) => void;
  onSortDirectionChange: (direction: SortDirection) => void;
  hideSortControl?: boolean;
  count: number;
  onImportFiles: () => void;
  onImportFolder: () => void;
  detailOpen?: boolean;
  onOpenTips?: () => void;
  mainView: MainView;
}

const segBase =
  'h-[26px] px-3 rounded-[2px] text-[length:var(--font-size-control)] font-medium cursor-pointer transition-colors';
const segActive = 'bg-[var(--accent)] text-[var(--accent-ink)]';
const segInactive = 'text-[var(--ink-2)] hover:text-[var(--ink)]';

export function Header({
  importTargetName,
  importTargetIsRoot = false,
  allFoldersCollapsed,
  onToggleAllFolders,
  view,
  onViewChange,
  sort,
  onSortChange,
  sortDirection,
  onSortDirectionChange,
  hideSortControl,
  count,
  onImportFiles,
  onImportFolder,
  mainView, onOpenTips,
  detailOpen = false,
}: Props) {
  const t = useT();
  const [importSource, setImportSource] = useState<'files' | 'folder'>('files');
  const [importMenuOpen, setImportMenuOpen] = useState(false);
  const [sortMenuOpen, setSortMenuOpen] = useState(false);

  const importRef = useRef<HTMLDivElement>(null);
  const sortRef = useRef<HTMLDivElement>(null);
  const importTriggerRef = useRef<HTMLButtonElement>(null);
  const sortTriggerRef = useRef<HTMLButtonElement>(null);
  const importKeyboard = useMenuKeyboard(importMenuOpen, setImportMenuOpen, importRef);
  const sortKeyboard = useMenuKeyboard(sortMenuOpen, setSortMenuOpen, sortRef);
  useDismissableMenu(importMenuOpen, setImportMenuOpen, importRef, importTriggerRef);
  useDismissableMenu(sortMenuOpen, setSortMenuOpen, sortRef, sortTriggerRef);
  useEffect(() => {
    setImportMenuOpen(false);
    setSortMenuOpen(false);
  }, [detailOpen, mainView, view]);

  const sortOptions: { value: SortKey; label: string }[] = [
    { value: 'name', label: t('sortName') },
    { value: 'imported', label: t('sortImported') },
    { value: 'modified', label: t('sortModified') },
    { value: 'size', label: t('sortSize') },
    { value: 'vol', label: t('sortVolume') },
    { value: 'viewed', label: t('sortLastViewed') },
  ];

  const directionLabels = sort === 'name' ? [t('sortNameAsc'), t('sortNameDesc')]
    : sort === 'size' ? [t('sortSizeAsc'), t('sortSizeDesc')]
    : sort === 'vol' ? [t('sortVolumeAsc'), t('sortVolumeDesc')]
    : [t('sortDateAsc'), t('sortDateDesc')];

  return (
    <header className="flex-none h-[54px] flex items-center gap-[18px] px-[14px] bg-[var(--panel)] border-b border-[var(--line)]">
      <div className="flex items-baseline gap-2 pr-1.5">
        <span className="text-[15px] font-bold tracking-[0.06em] uppercase">
          3MF Katalog
        </span>
        {(mainView === 'filament' || mainView === 'printers') && (
          <span className="font-mono-ui text-[11px] text-[var(--accent)] tracking-[0.08em]">
            {mainView === 'printers' ? 'PRINTER MANAGER' : 'MATERIAL MANAGER'}
          </span>
        )}
      </div>

      {mainView === 'catalog' && (
      <>
      <div ref={importRef} className="relative flex">
        <button
          ref={importTriggerRef}
          aria-haspopup="menu"
          onKeyDown={event => { if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) setSortMenuOpen(false); importKeyboard.onTriggerKeyDown(event); }}
          aria-expanded={importMenuOpen}
          onClick={() => { setSortMenuOpen(false); setImportMenuOpen((o) => !o); }}
          className="flex items-center gap-2 h-8 pl-[13px] pr-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[length:var(--font-size-body)] font-semibold cursor-pointer hover:brightness-110"
        >
          <span className="font-mono-ui text-sm leading-none">+</span>
          <span>{t('import')}</span>
          <span className="text-[length:var(--font-size-label)] leading-none">▾</span>
        </button>

        {importMenuOpen && (
          <div data-navigation-menu role="menu" onKeyDown={importKeyboard.onMenuKeyDown} className="absolute top-10 left-0 w-[300px] py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
            <div className="m-2 p-2 border border-dashed border-[var(--line-strong)] rounded bg-[var(--panel-2)] text-[12px] text-[var(--ink-2)] leading-relaxed" aria-live="polite">
              <b>{t(importSource === 'files' ? 'impMenuFiles' : 'impMenuFolder')}</b>{' '}
              {importSource === 'folder' ? t('impMenuFolderHint') : importTargetName ? <>{t('impMenuMove')} <b>{importTargetName}</b> {t(importTargetIsRoot ? 'impMenuRoot' : 'impMenuActive')}</> : t('impMenuStay')}
              <div className="font-mono-ui text-[11px] text-[var(--ink-3)]">{t(importSource === 'files' ? 'impMenuFixed' : 'impMenuNoMove')}</div>
            </div>
            <button
              role="menuitem" tabIndex={-1}
              onMouseEnter={() => setImportSource('files')} onFocus={() => setImportSource('files')}
              onClick={() => {
                setImportMenuOpen(false);
                onImportFiles();
              }}
              className="w-full text-left px-3 py-1.5 text-[length:var(--font-size-body)] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFilesOption')}
            </button>
            <button
              role="menuitem" tabIndex={-1}
              onMouseEnter={() => setImportSource('folder')} onFocus={() => setImportSource('folder')}
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolder();
              }}
              className="w-full text-left px-3 py-1.5 text-[length:var(--font-size-body)] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFolderOption')}
            </button>
          </div>
        )}
      </div>

      <div className="flex items-center gap-1.5" style={hideSortControl ? { display: 'none' } : undefined}>
        <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.1em] uppercase text-[var(--ink-3)]">
          {t('sortLabel')}
        </span>
        <div ref={sortRef} className="relative">
          <button
            ref={sortTriggerRef}
            onKeyDown={event => { if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) setImportMenuOpen(false); sortKeyboard.onTriggerKeyDown(event); }}
            aria-haspopup="menu"
            aria-expanded={sortMenuOpen}
            onClick={() => { setImportMenuOpen(false); setSortMenuOpen((o) => !o); }}
            className="h-[30px] px-2 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink)] text-[length:var(--font-size-body)] cursor-pointer flex items-center gap-1.5"
          >
            {sortOptions.find((o) => o.value === sort)?.label}
            <span className="font-mono-ui text-[var(--accent)]">{sortDirection === 'asc' ? '↑' : '↓'}</span>
            <span className="text-compact-label leading-none text-[var(--ink-3)]">▾</span>
          </button>
          {sortMenuOpen && (
            <div data-navigation-menu role="menu" onKeyDown={sortKeyboard.onMenuKeyDown} className="absolute top-9 left-0 min-w-[280px] w-max py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
              {sortOptions.map((opt) => (
                <button
                  key={opt.value}
                  role="menuitemradio" tabIndex={-1}
                  aria-checked={opt.value === sort}
                  onClick={() => {
                    onSortChange(opt.value, opt.value === sort ? sortDirection : defaultSortDirection(opt.value));
                  }}
                  className={`flex items-center gap-2 w-full text-left px-3 py-1.5 text-[13px] cursor-pointer ${
                    opt.value === sort
                      ? 'text-[var(--accent)] font-semibold bg-[var(--accent-soft)]'
                      : 'text-[var(--ink)] hover:bg-[var(--panel-2)]'
                  }`}
                >
                  <span aria-hidden="true" className="w-[14px] text-[var(--accent)]">{opt.value === sort ? '✓' : ''}</span>
                  <span>{opt.label}</span>
                  {opt.value === 'modified' && <small className="ml-auto font-mono-ui text-[10.5px] text-[var(--ink-3)]">{t('sortModifiedNew')}</small>}
                </button>
              ))}
              <div role="separator" className="h-px bg-[var(--line)] m-1" />
              <div role="group" aria-label={t('sortDirection')} className="flex gap-1 p-1.5">
                {(['asc', 'desc'] as const).map((direction, index) => (
                  <button key={direction} role="menuitemradio" tabIndex={-1} aria-checked={sortDirection === direction} aria-pressed={sortDirection === direction}
                    onClick={() => onSortDirectionChange(direction)}
                    className={`flex-1 whitespace-nowrap h-[30px] px-2 rounded border text-[12.5px] font-semibold cursor-pointer ${sortDirection === direction
                      ? 'border-[var(--accent)] text-[var(--accent)] bg-[var(--accent-soft)]'
                      : 'border-[var(--line)] text-[var(--ink-2)] hover:bg-[var(--panel-2)]'}`}>
                    {direction === 'asc' ? '↑' : '↓'} {directionLabels[index]}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
        <button
          onClick={() => onViewChange('grid')}
          className={`${segBase} ${view === 'grid' ? segActive : segInactive}`}
        >
          {t('viewGrid')}
        </button>
        <button
          onClick={() => onViewChange('groupedGrid')}
          className={`${segBase} ${view === 'groupedGrid' ? segActive : segInactive}`}
        >
          {t('viewFolder')}
        </button>
        <button
          onClick={() => onViewChange('groupedList')}
          className={`${segBase} ${view === 'groupedList' ? segActive : segInactive}`}
        >
          {t('viewList')}
        </button>
      </div>

      {(view === 'groupedGrid' || view === 'groupedList') && (
        <button type="button" onClick={onToggleAllFolders}
          className="h-[30px] px-2 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink)] text-[length:var(--font-size-body)] cursor-pointer flex items-center gap-1.5">
          <Icon name={allFoldersCollapsed ? 'expand-all' : 'collapse-all'} size={16} />
          {t(allFoldersCollapsed ? 'expandAllFolders' : 'collapseAllFolders')}
        </button>
      )}
      </>
      )}

      <div className="flex-1" />

      {mainView === 'catalog' && (
        <span className="shrink-0 font-mono-ui text-[11px] text-[var(--ink-3)]">
          {formatCount(t('filesCount'), count)}
        </span>
      )}

      <button onClick={onOpenTips} aria-label={t('keyboardTipsTitle')} title={t('keyboardTipsTitle')}
        className="shrink-0 w-8 h-8 rounded-full border border-[var(--line-strong)] text-[var(--ink-2)] hover:text-[var(--accent)] cursor-pointer">?</button>
    </header>
  );
}
