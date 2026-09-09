import { useState } from 'react';
import type { ViewMode, SortKey } from '../types';
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
}: Props) {
  const t = useT();
  const { language, setLanguage } = useLanguage();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [importMenuOpen, setImportMenuOpen] = useState(false);

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

      <div className="flex-1" />

      <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">
        {formatCount(t('filesCount'), count)}
      </span>

      <div className="relative">
        <button
          onClick={() => setSettingsOpen((o) => !o)}
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
          </div>
        )}
      </div>
    </header>
  );
}
