import { useState } from 'react';
import type { ViewMode, SortKey } from '../types';
import { formatCount } from '../i18n/types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  view: ViewMode;
  onViewChange: (v: ViewMode) => void;
  sort: SortKey;
  onSortChange: (s: SortKey) => void;
  hideSortControl?: boolean;
  count: number;
  onImportFiles: () => void;
  onImportFolder: () => void;
  onImportFolderAsCollection: () => void;
  mainView: 'catalog' | 'filament' | 'trash';
}

const segBase =
  'h-[26px] px-3 rounded-[2px] text-[length:var(--font-size-control)] font-medium cursor-pointer transition-colors';
const segActive = 'bg-[var(--accent)] text-[var(--accent-ink)]';
const segInactive = 'text-[var(--ink-2)] hover:text-[var(--ink)]';

export function Header({
  view,
  onViewChange,
  sort,
  onSortChange,
  hideSortControl,
  count,
  onImportFiles,
  onImportFolder,
  onImportFolderAsCollection,
  mainView,
}: Props) {
  const t = useT();
  const [importMenuOpen, setImportMenuOpen] = useState(false);
  const [sortMenuOpen, setSortMenuOpen] = useState(false);

  const sortOptions: { value: SortKey; label: string }[] = [
    { value: 'name', label: t('sortName') },
    { value: 'date', label: t('sortDate') },
    { value: 'size', label: t('sortSize') },
    { value: 'vol', label: t('sortVolume') },
    { value: 'viewed', label: t('sortLastViewed') },
  ];

  return (
    <header className="flex-none h-[54px] flex items-center gap-[18px] px-[14px] bg-[var(--panel)] border-b border-[var(--line)]">
      <div className="flex items-baseline gap-2 pr-1.5">
        <span className="text-[15px] font-bold tracking-[0.06em] uppercase">
          3MF Katalog
        </span>
        <span className="font-mono-ui text-[11px] text-[var(--accent)] tracking-[0.08em]">
          MANAGER
        </span>
      </div>

      {mainView === 'catalog' && (
      <>
      <div className="relative flex">
        <button
          onClick={() => setImportMenuOpen((o) => !o)}
          className="flex items-center gap-2 h-8 pl-[13px] pr-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[length:var(--font-size-body)] font-semibold cursor-pointer hover:brightness-110"
        >
          <span className="font-mono-ui text-sm leading-none">+</span>
          <span>{t('import')}</span>
          <span className="text-[length:var(--font-size-label)] leading-none">▾</span>
        </button>

        {importMenuOpen && (
          <div className="absolute top-10 left-0 w-[176px] py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFiles();
              }}
              className="w-full text-left px-3 py-1.5 text-[length:var(--font-size-body)] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFilesOption')}
            </button>
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolder();
              }}
              className="w-full text-left px-3 py-1.5 text-[length:var(--font-size-body)] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFolderOption')}
            </button>
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolderAsCollection();
              }}
              className="w-full text-left px-3 py-1.5 text-[length:var(--font-size-body)] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFolderAsCollectionOption')}
            </button>
          </div>
        )}
      </div>

      <div className="flex items-center gap-1.5" style={hideSortControl ? { display: 'none' } : undefined}>
        <span className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.1em] uppercase text-[var(--ink-3)]">
          {t('sortLabel')}
        </span>
        <div className="relative">
          <button
            onClick={() => setSortMenuOpen((o) => !o)}
            className="h-[30px] px-2 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink)] text-[length:var(--font-size-body)] cursor-pointer flex items-center gap-1.5"
          >
            {sortOptions.find((o) => o.value === sort)?.label}
            <span className="text-[9px] leading-none text-[var(--ink-3)]">▾</span>
          </button>
          {sortMenuOpen && (
            <div className="absolute top-9 left-0 w-[176px] py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
              {sortOptions.map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => {
                    onSortChange(opt.value);
                    setSortMenuOpen(false);
                  }}
                  className={`w-full text-left px-3 py-1.5 text-[13px] cursor-pointer ${
                    opt.value === sort
                      ? 'text-[var(--accent)] font-semibold bg-[var(--accent-soft)]'
                      : 'text-[var(--ink)] hover:bg-[var(--panel-2)]'
                  }`}
                >
                  {opt.label}
                </button>
              ))}
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

      </>
      )}

      <div className="flex-1" />

      {mainView === 'catalog' && (
        <span className="shrink-0 font-mono-ui text-[11px] text-[var(--ink-3)]">
          {formatCount(t('filesCount'), count)}
        </span>
      )}

    </header>
  );
}
