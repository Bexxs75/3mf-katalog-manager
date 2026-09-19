import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { SlicerConfig } from '../types';
import type { ThemeSetting } from '../hooks/useTheme';
import type { UiDensity } from '../hooks/UiDensityContext';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import type { Language } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';

type MainView = 'catalog' | 'filament' | 'trash';

interface Props {
  mainView: MainView;
  onMainViewChange: (v: MainView) => void;
  trashCount: number;
  settingsOpen: boolean;
  onSettingsOpenChange: (open: boolean) => void;
  themeSetting: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  uiDensity: UiDensity;
  onUiDensityChange: (d: UiDensity) => void;
  displayPreference: DisplayPreference;
  onDisplayPreferenceChange: (p: DisplayPreference) => void;
  slicers: SlicerConfig[];
  primarySlicerId: string | null;
  onAddSlicer: (name: string, path: string) => void;
  onRemoveSlicer: (id: string) => void;
  onSetPrimarySlicer: (id: string) => void;
  onScanCatalogIssues: () => void;
  cleanupScanning: boolean;
  cleanupError: string | null;
  onExportCatalog: () => void;
  onImportCatalog: () => void;
  catalogBackupError: string | null;
  catalogBaseDir: string | null;
  onOpenCatalogSetup: () => void;
  updateInfo: {
    currentVersion: string;
    latestVersion: string;
    updateAvailable: boolean;
    checking: boolean;
    checkNow: () => void;
    download: () => void;
  };
}

const railBtnBase =
  'relative w-[42px] h-[42px] rounded-[10px] border border-transparent grid place-items-center cursor-pointer text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)]';
const railBtnActive =
  'text-[var(--accent)] bg-[var(--accent-soft)] border-[color-mix(in_oklch,var(--accent)_30%,transparent)]';

const segBase =
  'h-[26px] px-3 rounded-[2px] text-[length:var(--font-size-control)] font-medium cursor-pointer transition-colors';
const segActive = 'bg-[var(--accent)] text-[var(--accent-ink)]';
const segInactive = 'text-[var(--ink-2)] hover:text-[var(--ink)]';

const LANGUAGE_LABELS: Record<Language, string> = {
  de: 'Deutsch',
  en: 'English',
  es: 'Español',
  fr: 'Français',
};

export function Rail({
  mainView,
  onMainViewChange,
  trashCount,
  settingsOpen,
  onSettingsOpenChange,
  themeSetting,
  onThemeChange,
  uiDensity,
  onUiDensityChange,
  displayPreference,
  onDisplayPreferenceChange,
  slicers,
  primarySlicerId,
  onAddSlicer,
  onRemoveSlicer,
  onSetPrimarySlicer,
  onScanCatalogIssues,
  cleanupScanning,
  cleanupError,
  onExportCatalog,
  onImportCatalog,
  catalogBackupError,
  catalogBaseDir,
  onOpenCatalogSetup,
  updateInfo,
}: Props) {
  const t = useT();
  const { language, setLanguage } = useLanguage();
  const [pendingSlicerPath, setPendingSlicerPath] = useState<string | null>(null);
  const [pendingSlicerName, setPendingSlicerName] = useState('');
  const [confirmImportCatalog, setConfirmImportCatalog] = useState(false);
  const [activeSettingsTab, setActiveSettingsTab] = useState<'general' | 'slicer' | 'catalog' | 'info'>('general');

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
    <nav className="flex-none w-[60px] flex flex-col items-center pt-3.5 pb-2.5 bg-[var(--panel-2)] border-r border-[var(--line)]">
      <div className="flex flex-col gap-1.5">
        <button
          onClick={() => onMainViewChange('catalog')}
          title={t('railCatalog')}
          aria-label={t('railCatalog')}
          className={`${railBtnBase} ${mainView === 'catalog' ? railBtnActive : ''}`}
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="currentColor">
            <rect x="3" y="3" width="7" height="7" rx="1.5" />
            <rect x="14" y="3" width="7" height="7" rx="1.5" />
            <rect x="3" y="14" width="7" height="7" rx="1.5" />
            <rect x="14" y="14" width="7" height="7" rx="1.5" />
          </svg>
        </button>
        <button
          onClick={() => onMainViewChange('filament')}
          title={t('railFilament')}
          aria-label={t('railFilament')}
          className={`${railBtnBase} ${mainView === 'filament' ? railBtnActive : ''}`}
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="currentColor" fillRule="evenodd">
            <path d="M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 1 0 0-17ZM12 9a3 3 0 1 0 0 6 3 3 0 1 0 0-6Z" />
          </svg>
        </button>
        <button
          onClick={() => onMainViewChange('trash')}
          title={t('trashHeading')}
          aria-label={t('trashHeading')}
          className={`${railBtnBase} ${mainView === 'trash' ? railBtnActive : ''}`}
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="currentColor">
            <path d="M6 19a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V7H6v12ZM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4Z" />
          </svg>
          {trashCount > 0 && (
            <span className="absolute -top-1 -right-1 min-w-[15px] h-[15px] px-[3px] rounded-full bg-[var(--accent)] text-[var(--accent-ink)] text-[9px] font-bold font-mono-ui grid place-items-center">
              {trashCount}
            </span>
          )}
        </button>
      </div>

      <div className="flex-1" />

      <div className="relative shrink-0">
        <button
          onClick={() => onSettingsOpenChange(!settingsOpen)}
          title={t('settingsTitle')}
          aria-label={t('settingsTitle')}
          className={`${railBtnBase} ${railBtnActive}`}
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="currentColor" fillRule="evenodd">
            <path d="M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.5.5 0 0 0 .12-.64l-1.92-3.32a.5.5 0 0 0-.6-.22l-2.39.96a7.3 7.3 0 0 0-1.62-.94l-.36-2.54a.5.5 0 0 0-.5-.42h-3.84a.5.5 0 0 0-.5.42l-.36 2.54c-.59.24-1.13.56-1.62.94l-2.39-.96a.5.5 0 0 0-.6.22L1.66 8.88a.5.5 0 0 0 .12.64l2.03 1.58a7.5 7.5 0 0 0 0 1.88l-2.03 1.58a.5.5 0 0 0-.12.64l1.92 3.32a.5.5 0 0 0 .6.22l2.39-.96c.49.38 1.03.7 1.62.94l.36 2.54a.5.5 0 0 0 .5.42h3.84a.5.5 0 0 0 .5-.42l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96a.5.5 0 0 0 .6-.22l1.92-3.32a.5.5 0 0 0-.12-.64l-2.03-1.58ZM12 15.6a3.6 3.6 0 1 1 0-7.2 3.6 3.6 0 0 1 0 7.2Z" />
          </svg>
        </button>

        {settingsOpen && (
          <div className="absolute bottom-0 left-12 w-[268px] p-[14px] bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40 max-h-[calc(100vh-80px)] overflow-y-auto">
            <div className="font-mono-ui text-[length:var(--font-size-meta)] tracking-[0.12em] uppercase text-[var(--ink-3)] mb-2.5">
              {t('settingsTitle')}
            </div>
            <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)] mb-3">
              {([
                ['general', t('settingsTabGeneral')],
                ['slicer', t('settingsTabSlicer')],
                ['catalog', t('settingsTabCatalog')],
                ['info', t('settingsTabInfo')],
              ] as const).map(([key, label]) => (
                <button
                  key={key}
                  onClick={() => setActiveSettingsTab(key)}
                  className={`${segBase} flex-1 !h-[24px] !px-1 text-[11.5px] ${activeSettingsTab === key ? segActive : segInactive}`}
                >
                  {label}
                </button>
              ))}
            </div>

            {activeSettingsTab === 'general' && (
              <>
                <div className="text-[length:var(--font-size-body)] font-semibold mb-2">{t('appearanceTitle')}</div>
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

                <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)] mt-2.5">
                  {(['compact', 'comfort'] as UiDensity[]).map((opt) => (
                    <button
                      key={opt}
                      onClick={() => onUiDensityChange(opt)}
                      className={`${segBase} flex-1 ${uiDensity === opt ? segActive : segInactive}`}
                    >
                      {opt === 'compact' ? t('densityCompact') : t('densityComfort')}
                    </button>
                  ))}
                </div>
                <div className="mt-2 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">
                  {uiDensity === 'compact' ? t('densityDescriptionCompact') : t('densityDescriptionComfort')}
                </div>

                <div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('displayPreferenceTitle')}</div>
                <div className="flex p-0.5 gap-0.5 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
                  {(['thumbnail', 'render'] as DisplayPreference[]).map((opt) => (
                    <button
                      key={opt}
                      onClick={() => onDisplayPreferenceChange(opt)}
                      className={`${segBase} flex-1 ${displayPreference === opt ? segActive : segInactive}`}
                    >
                      {opt === 'thumbnail' ? t('displayPreferenceThumbnail') : t('displayPreferenceRender')}
                    </button>
                  ))}
                </div>
                <div className="mt-2 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">
                  {displayPreference === 'thumbnail'
                    ? t('displayPreferenceDescriptionThumbnail')
                    : t('displayPreferenceDescriptionRender')}
                </div>

                <div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('languageTitle')}</div>
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
              </>
            )}

            {activeSettingsTab === 'slicer' && (
              <>
                <div className="text-[length:var(--font-size-body)] font-semibold mb-2">{t('slicerSectionTitle')}</div>
                {slicers.length === 0 ? (
                  <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
                    {t('noSlicersConfigured')}
                  </div>
                ) : (
                  <div className="flex flex-col gap-1.5">
                    {slicers.map((s) => {
                      const isPrimary = s.id === primarySlicerId;
                      return (
                        <div key={s.id} className="flex items-center gap-2">
                          <button
                            onClick={() => onSetPrimarySlicer(s.id)}
                            aria-label={t('setPrimarySlicerAria')}
                            className="flex-none w-3.5 h-3.5 rounded-full border cursor-pointer grid place-items-center"
                            style={{ borderColor: isPrimary ? 'var(--accent)' : 'var(--line-strong)' }}
                          >
                            {isPrimary && <span className="w-1.5 h-1.5 rounded-full" style={{ background: 'var(--accent)' }} />}
                          </button>
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-1.5">
                              <div className="text-[length:var(--font-size-title)] text-[var(--ink)] truncate">{s.name}</div>
                              {isPrimary && (
                                <span className="flex-none font-mono-ui text-[9px] tracking-[0.08em] uppercase px-1.5 rounded-full" style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}>
                                  {t('slicerPrimaryChip')}
                                </span>
                              )}
                              {s.source === 'auto' && (
                                <span className="flex-none font-mono-ui text-[9px] tracking-[0.08em] uppercase text-[var(--ink-3)]">
                                  {t('slicerAutoDetectedLabel')}
                                </span>
                              )}
                            </div>
                            <div className="font-mono-ui text-[length:var(--font-size-meta)] text-[var(--ink-3)] truncate">
                              {s.path}
                            </div>
                          </div>
                          <span
                            onClick={() => onRemoveSlicer(s.id)}
                            aria-label={t('removeSlicerAria')}
                            className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                          >
                            ✕
                          </span>
                        </div>
                      );
                    })}
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
                      className="flex-1 h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[length:var(--font-size-title)]"
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
              </>
            )}

            {activeSettingsTab === 'catalog' && (
              <>
                <div className="text-[length:var(--font-size-body)] font-semibold mb-2">{t('catalogCleanupTitle')}</div>
                <button
                  onClick={onScanCatalogIssues}
                  disabled={cleanupScanning}
                  className={`h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] ${
                    cleanupScanning ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]'
                  }`}
                >
                  {cleanupScanning ? t('catalogCleanupScanning') : t('catalogCleanupScanButton')}
                </button>
                {cleanupError && (
                  <div className="mt-1.5 font-mono-ui text-[length:var(--font-size-meta)] text-[var(--accent)] break-words">
                    {t('catalogCleanupError')} {cleanupError}
                  </div>
                )}

                <div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('catalogBackupTitle')}</div>
                <button
                  onClick={onExportCatalog}
                  className="h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                >
                  {t('exportCatalogButton')}
                </button>
                {confirmImportCatalog ? (
                  <div className="mt-1.5 flex flex-col gap-1.5">
                    <div className="font-mono-ui text-[10.5px] text-[var(--ink-2)]">
                      {t('importCatalogConfirmQuestion')}
                    </div>
                    <div className="flex gap-1.5">
                      <button
                        onClick={() => {
                          setConfirmImportCatalog(false);
                          onImportCatalog();
                        }}
                        className="flex-1 h-7 rounded-[3px] border border-red-400 bg-transparent text-red-400 text-[12px] cursor-pointer hover:bg-red-400/10"
                      >
                        {t('importCatalogConfirmYes')}
                      </button>
                      <button
                        onClick={() => setConfirmImportCatalog(false)}
                        className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)]"
                      >
                        {t('cancel')}
                      </button>
                    </div>
                  </div>
                ) : (
                  <button
                    onClick={() => setConfirmImportCatalog(true)}
                    className="mt-1.5 h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                  >
                    {t('importCatalogButton')}
                  </button>
                )}
                {catalogBackupError && (
                  <div className="mt-1.5 font-mono-ui text-[length:var(--font-size-meta)] text-[var(--accent)] break-words">
                    {catalogBackupError}
                  </div>
                )}

                <div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('catalogBaseDirSectionTitle')}</div>
                <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)] truncate mb-1.5">
                  {catalogBaseDir ?? t('catalogBaseDirNotSet')}
                </div>
                <div className="flex gap-1.5">
                  <button
                    onClick={onOpenCatalogSetup}
                    className="flex-1 h-7 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                  >
                    {catalogBaseDir ? t('catalogBaseDirChangeButton') : t('catalogBaseDirSetupButton')}
                  </button>
                  {catalogBaseDir && (
                    <button
                      onClick={() => invoke('open_in_file_manager', { path: catalogBaseDir })}
                      className="flex-1 h-7 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                    >
                      {t('catalogBaseDirOpenButton')}
                    </button>
                  )}
                </div>
              </>
            )}

            {activeSettingsTab === 'info' && (
              <>
                <div className="text-center mb-3">
                  <div className="text-[13.5px] font-bold">3MF Katalog Manager</div>
                  <div className="mt-0.5 font-mono-ui text-[10.5px] text-[var(--ink-3)]">
                    {t('infoAppVersionLabel').replace('{version}', updateInfo.currentVersion)}
                  </div>
                  {updateInfo.updateAvailable ? (
                    <div className="mt-2.5 p-2.5 rounded-[5px]" style={{ background: 'var(--good-soft, var(--accent-soft))', border: '1px solid var(--line)' }}>
                      <div className="text-[12.5px] font-semibold" style={{ color: 'var(--good, var(--accent))' }}>
                        {t('infoUpdateAvailableLabel').replace('{version}', updateInfo.latestVersion)}
                      </div>
                      <button
                        onClick={updateInfo.download}
                        className="mt-2 h-7 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12px] font-semibold cursor-pointer"
                      >
                        {t('infoViewReleaseNotes')}
                      </button>
                    </div>
                  ) : (
                    <div className="mt-2 font-mono-ui text-[10.5px] text-[var(--ink-3)]">{t('infoUpToDateLabel')}</div>
                  )}
                </div>
                <button
                  onClick={updateInfo.checkNow}
                  disabled={updateInfo.checking}
                  className={`h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] ${
                    updateInfo.checking ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]'
                  }`}
                >
                  {updateInfo.checking ? t('infoCheckingForUpdate') : t('infoCheckForUpdateButton')}
                </button>
                <div className="flex justify-between text-[11.5px] py-2 border-t border-[var(--line)] mt-3 text-[var(--ink-2)]">
                  <span>{t('infoSourceCodeLabel')}</span>
                  <span
                    onClick={() => invoke('open_release_url', { url: 'https://github.com/Bexxs75/3mf-katalog-manager' }).catch(() => {})}
                    className="cursor-pointer hover:text-[var(--accent)]"
                  >
                    GitHub
                  </span>
                </div>
                <div className="flex justify-between text-[11.5px] py-1 text-[var(--ink-2)]">
                  <span>{t('infoLicenseLabel')}</span>
                  <span>MIT</span>
                </div>
              </>
            )}
          </div>
        )}
      </div>
    </nav>
  );
}
