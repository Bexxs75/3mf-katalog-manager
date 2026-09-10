import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ViewMode, SortKey, SlicerConfig } from '../types';
import type { ThemeSetting } from '../hooks/useTheme';
import type { Language } from '../i18n/types';
import { formatCount } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';

interface Props {
  view: ViewMode;
  onViewChange: (v: ViewMode) => void;
  sort: SortKey;
  onSortChange: (s: SortKey) => void;
  count: number;
  themeSetting: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  onImportFiles: () => void;
  onImportFolder: () => void;
  cloudDriveConnected: boolean;
  onImportFromCloud: () => void;
  settingsOpen: boolean;
  onSettingsOpenChange: (open: boolean) => void;
  slicers: SlicerConfig[];
  onAddSlicer: (name: string, path: string) => void;
  onRemoveSlicer: (id: string) => void;
  mainView: 'catalog' | 'filament';
  onMainViewChange: (view: 'catalog' | 'filament') => void;
}

const segBase =
  'h-[26px] px-3 rounded-[2px] text-[12.5px] font-medium cursor-pointer transition-colors';
const segActive = 'bg-[var(--accent)] text-[var(--accent-ink)]';
const segInactive = 'text-[var(--ink-2)] hover:text-[var(--ink)]';

const LANGUAGE_LABELS: Record<Language, string> = {
  de: 'Deutsch',
  en: 'English',
  es: 'Español',
  fr: 'Français',
};

export function Header({
  view,
  onViewChange,
  sort,
  onSortChange,
  count,
  themeSetting,
  onThemeChange,
  onImportFiles,
  onImportFolder,
  cloudDriveConnected,
  onImportFromCloud,
  settingsOpen,
  onSettingsOpenChange,
  slicers,
  onAddSlicer,
  onRemoveSlicer,
  mainView,
  onMainViewChange,
}: Props) {
  const t = useT();
  const { language, setLanguage } = useLanguage();
  const [importMenuOpen, setImportMenuOpen] = useState(false);
  const [pendingSlicerPath, setPendingSlicerPath] = useState<string | null>(null);
  const [pendingSlicerName, setPendingSlicerName] = useState('');

  const handlePickSlicer = () => {
    invoke<string | null>('pick_slicer_executable').then((path) => {
      if (!path) return;
      const fileName = path.split(/[/\\]/).pop() ?? path;
      const suggested = fileName.replace(/\.[^./\\]+$/, '');
      setPendingSlicerPath(path);
      setPendingSlicerName(suggested);
    });
  };

  const confirmAddSlicer = () => {
    if (!pendingSlicerPath || !pendingSlicerName.trim()) return;
    onAddSlicer(pendingSlicerName.trim(), pendingSlicerPath);
    setPendingSlicerPath(null);
    setPendingSlicerName('');
  };

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
          onClick={() => {
            setImportMenuOpen(false);
            onImportFiles();
          }}
          className="flex items-center gap-2 h-8 pl-[13px] pr-3 rounded-l-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[13px] font-semibold cursor-pointer hover:brightness-110"
        >
          <span className="font-mono-ui text-sm leading-none">+</span>
          <span>{t('import')}</span>
        </button>
        <button
          onClick={() => setImportMenuOpen((o) => !o)}
          aria-label={t('importMoreOptionsAria')}
          className="flex items-center justify-center w-6 h-8 rounded-r-[3px] border border-l-0 border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] cursor-pointer hover:brightness-110"
        >
          <span className="text-[9px] leading-none">▾</span>
        </button>

        {importMenuOpen && (
          <div className="absolute top-10 left-0 w-[176px] py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFiles();
              }}
              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFilesOption')}
            </button>
            <button
              onClick={() => {
                setImportMenuOpen(false);
                onImportFolder();
              }}
              className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              {t('importFolderOption')}
            </button>
            {cloudDriveConnected && (
              <button
                onClick={() => {
                  setImportMenuOpen(false);
                  onImportFromCloud();
                }}
                className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
              >
                {t('importFromCloudOption')}
              </button>
            )}
          </div>
        )}
      </div>

      <div className="flex items-center gap-1.5">
        <span className="font-mono-ui text-[10px] tracking-[0.1em] uppercase text-[var(--ink-3)]">
          {t('sortLabel')}
        </span>
        <select
          value={sort}
          onChange={(e) => onSortChange(e.target.value as SortKey)}
          className="h-[30px] px-2 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink)] text-[13px] cursor-pointer"
        >
          <option value="name">{t('sortName')}</option>
          <option value="date">{t('sortDate')}</option>
          <option value="size">{t('sortSize')}</option>
          <option value="vol">{t('sortVolume')}</option>
          <option value="viewed">{t('sortLastViewed')}</option>
        </select>
      </div>

      <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
        <button
          onClick={() => onViewChange('grid')}
          className={`${segBase} ${view === 'grid' ? segActive : segInactive}`}
        >
          {t('viewGrid')}
        </button>
        <button
          onClick={() => onViewChange('list')}
          className={`${segBase} ${view === 'list' ? segActive : segInactive}`}
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

      <button
        onClick={() => onMainViewChange(mainView === 'catalog' ? 'filament' : 'catalog')}
        className="shrink-0 h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] text-[13px] font-semibold cursor-pointer hover:text-[var(--ink)] hover:border-[var(--line-strong)]"
      >
        {mainView === 'catalog' ? t('filamentNavButton') : t('filamentBackToCatalogButton')}
      </button>

      <div className="relative shrink-0">
        <button
          onClick={() => onSettingsOpenChange(!settingsOpen)}
          className="w-8 h-8 grid place-items-center rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] text-[15px] cursor-pointer hover:text-[var(--ink)] hover:border-[var(--line-strong)]"
        >
          ⚙
        </button>

        {settingsOpen && (
          <div className="absolute top-10 right-0 w-[268px] p-[14px] bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
            <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] mb-2.5">
              {t('settingsTitle')}
            </div>
            <div className="text-[13px] font-semibold mb-2">{t('appearanceTitle')}</div>
            <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
              {(['system', 'light', 'dark'] as ThemeSetting[]).map((opt) => (
                <button
                  key={opt}
                  onClick={() => onThemeChange(opt)}
                  className={`${segBase} flex-1 ${themeSetting === opt ? segActive : segInactive}`}
                >
                  {opt === 'system' ? t('themeSystem') : opt === 'light' ? t('themeLight') : t('themeDark')}
                </button>
              ))}
            </div>
            <div className="mt-2 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">
              {themeSetting === 'system'
                ? t('themeDescriptionSystem')
                : t('themeDescriptionManual').replace(
                    '{mode}',
                    themeSetting === 'light' ? t('themeLight') : t('themeDark'),
                  )}
            </div>

            <div className="text-[13px] font-semibold mt-4 mb-2">{t('languageTitle')}</div>
            <div className="grid grid-cols-2 gap-0.5 p-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
              {(['de', 'en', 'es', 'fr'] as Language[]).map((lang) => (
                <button
                  key={lang}
                  onClick={() => setLanguage(lang)}
                  className={`${segBase} ${language === lang ? segActive : segInactive}`}
                >
                  {LANGUAGE_LABELS[lang]}
                </button>
              ))}
            </div>

            <div className="text-[13px] font-semibold mt-4 mb-2">{t('slicerSectionTitle')}</div>
            {slicers.length === 0 ? (
              <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
                {t('noSlicersConfigured')}
              </div>
            ) : (
              <div className="flex flex-col gap-1.5">
                {slicers.map((s) => (
                  <div key={s.id} className="flex items-center gap-2">
                    <div className="flex-1 min-w-0">
                      <div className="text-[12.5px] text-[var(--ink)] truncate">{s.name}</div>
                      <div className="font-mono-ui text-[10px] text-[var(--ink-3)] truncate">
                        {s.path}
                      </div>
                    </div>
                    <span
                      onClick={() => onRemoveSlicer(s.id)}
                      aria-label={t('removeSlicerAria')}
                      className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                    >
                      ✕
                    </span>
                  </div>
                ))}
              </div>
            )}
            {pendingSlicerPath ? (
              <div className="flex items-center gap-1.5 mt-2">
                <input
                  value={pendingSlicerName}
                  onChange={(e) => setPendingSlicerName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') confirmAddSlicer();
                    if (e.key === 'Escape') {
                      setPendingSlicerPath(null);
                      setPendingSlicerName('');
                    }
                  }}
                  autoFocus
                  className="flex-1 h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[12.5px]"
                />
                <button
                  onClick={confirmAddSlicer}
                  className="h-7 px-2.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[11.5px] font-semibold cursor-pointer"
                >
                  {t('confirmSlicerName')}
                </button>
                <button
                  onClick={() => {
                    setPendingSlicerPath(null);
                    setPendingSlicerName('');
                  }}
                  aria-label={t('cancel')}
                  className="flex-none w-7 h-7 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                >
                  ✕
                </button>
              </div>
            ) : (
              <button
                onClick={handlePickSlicer}
                className="mt-2 h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                + {t('addSlicer')}
              </button>
            )}
          </div>
        )}
      </div>
    </header>
  );
}
