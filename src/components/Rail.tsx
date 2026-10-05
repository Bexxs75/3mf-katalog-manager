import { useImportLock } from '../hooks/ImportLockContext';
import { Icon } from './Icon';
import { CatalogResetSection } from './CatalogResetSection';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { MainView, SlicerConfig } from '../types';
import type { ThemeSetting } from '../hooks/useTheme';
import type { UiDensity } from '../hooks/UiDensityContext';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import type { Language } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { DiagnosticsSettings } from '../diagnostics/DiagnosticsSettings';
import { InfoFolders } from '../diagnostics/InfoFolders';
import type { AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import type { UpdaterView } from '../hooks/useUpdater';
import { UpdatePanel } from './UpdatePanel';

interface Props {
  detailPanel?: 'auto' | 'pinned';
  onDetailPanelChange?: (mode: 'auto' | 'pinned') => void;
  singleKeyShortcuts?: boolean;
  onSingleKeyShortcutsChange?: (enabled: boolean) => void;
  onOpenTips?: () => void;
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
  onAddSlicer: () => void;
  addSlicerError: AppError | null;
  onRemoveSlicer: (id: string) => void;
  onSetPrimarySlicer: (id: string) => void;
  onScanCatalogIssues: () => void;
  cleanupScanning: boolean;
  cleanupError: AppError | null;
  onExportCatalog: () => Promise<boolean>;
  catalogModelCount: number;
  catalogFolderCount: number;
  onCatalogReset: () => void;
  onImportCatalog: () => void;
  onClearCatalogBackupError?: () => void;
  catalogBackupError: AppError | null;
  catalogBaseDir: string | null;
  onOpenCatalogSetup: () => void;
  update: UpdaterView;
}

const railBtnBase =
  'relative w-[42px] h-[42px] rounded-[10px] border border-transparent grid place-items-center cursor-pointer text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]';
const railBtnActive =
  'text-[var(--accent)] bg-[var(--accent-soft)] border-[color-mix(in_oklch,var(--accent)_30%,transparent)]';

const segBase =
  'h-[26px] px-3 rounded-[2px] text-[length:var(--font-size-control)] font-medium cursor-pointer transition-colors';
const segActive = 'bg-[var(--accent)] text-[var(--accent-ink)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)]';
const segInactive = 'text-[var(--ink-2)] hover:text-[var(--ink)]';

const LANGUAGE_LABELS: Record<Language, string> = {
  de: 'Deutsch',
  en: 'English',
  es: 'Español',
  fr: 'Français',
};

export function Rail({
  detailPanel = 'auto', onDetailPanelChange,
  singleKeyShortcuts = true, onSingleKeyShortcutsChange, onOpenTips,
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
  addSlicerError,
  onRemoveSlicer,
  onSetPrimarySlicer,
  onScanCatalogIssues,
  cleanupScanning,
  cleanupError,
  onExportCatalog,
  catalogModelCount,
  catalogFolderCount,
  onCatalogReset,
  onImportCatalog,
  onClearCatalogBackupError,
  catalogBackupError,
  catalogBaseDir,
  onOpenCatalogSetup,
  update,
}: Props) {
  const { lockProps } = useImportLock();
  const t = useT();
  const { language, setLanguage } = useLanguage();
  const [confirmImportCatalog, setConfirmImportCatalog] = useState(false);
  const [activeSettingsTab, setActiveSettingsTab] = useState<'general' | 'slicer' | 'catalog' | 'info'>('general');

  const railRef = useRef<HTMLElement>(null);
  const settingsWrapperRef = useRef<HTMLDivElement>(null);
  const [panelOffset, setPanelOffset] = useState({ left: 0, bottom: 10 });
  useLayoutEffect(() => {
    if (!settingsOpen) return;
    const measure = () => {
      const rail = railRef.current?.getBoundingClientRect();
      const wrapper = settingsWrapperRef.current?.getBoundingClientRect();
      if (!rail || !wrapper) return;
      // Measure the containing block: the rail border affects centering,
      // and its bottom padding already contributes to the viewport gap.
      setPanelOffset({ left: rail.right - wrapper.left, bottom: wrapper.bottom - window.innerHeight + 10 });
    };
    measure();
    window.addEventListener('resize', measure);
    return () => window.removeEventListener('resize', measure);
  }, [settingsOpen]);

  const settingsButtonRef = useRef<HTMLButtonElement>(null);
  const settingsPanelRef = useRef<HTMLDivElement>(null);
  const keyboardOpen = useRef(false);
  useEffect(() => {
    if (settingsOpen && keyboardOpen.current) {
      settingsPanelRef.current?.querySelector<HTMLElement>('button:not(:disabled), input:not(:disabled)')?.focus();
      keyboardOpen.current = false;
    }
  }, [settingsOpen]);

  return (
    <nav ref={railRef} className="flex-none w-[60px] flex flex-col items-center pt-3.5 pb-2.5 bg-[var(--panel-2)] border-r border-[var(--line)]">
      <div className="flex flex-col gap-1.5">
        <button
          onClick={() => onMainViewChange('catalog')}
          title={t('railCatalog')}
          aria-label={t('railCatalog')}
          className={`${railBtnBase} ${mainView === 'catalog' ? railBtnActive : ''}`}
        >
          <Icon name="catalog" />
        </button>
        <button
          onClick={() => onMainViewChange('filament')}
          title={t('railFilament')}
          aria-label={t('railFilament')}
          className={`${railBtnBase} ${mainView === 'filament' ? railBtnActive : ''}`}
        >
          <Icon name="spool" />
        </button>
        <button onClick={() => onMainViewChange('printers')} title={t('railPrinters')} aria-label={t('railPrinters')}
          className={`${railBtnBase} ${mainView === 'printers' ? railBtnActive : ''}`}>
          <Icon name="printer" />
        </button>
        <button
          onClick={() => onMainViewChange('trash')}
          title={t('trashHeading')}
          aria-label={t('trashHeading')}
          className={`${railBtnBase} ${mainView === 'trash' ? railBtnActive : ''}`}
        >
          <Icon name="trash" />
          {trashCount > 0 && (
            <span className="absolute -top-1 -right-1 min-w-[15px] h-[15px] px-[3px] rounded-full bg-[var(--accent)] text-[var(--accent-ink)] text-compact-label font-semibold tabular-nums grid place-items-center">
              {trashCount}
            </span>
          )}
        </button>
      </div>

      <div className="flex-1" />

      <div ref={settingsWrapperRef} className="relative shrink-0">
        <button
          ref={settingsButtonRef}
          onClick={(event) => {
            keyboardOpen.current = event.detail === 0 && !settingsOpen;
            onSettingsOpenChange(!settingsOpen);
          }}
          title={t('settingsTitle')}
          aria-label={t('settingsTitle')}
          className={`${railBtnBase} ${railBtnActive}`}
        >
          <Icon name="settings" />
        </button>

        {settingsOpen && (
          <div ref={settingsPanelRef} onKeyDown={(event) => {
            if (document.querySelector('[role="dialog"][aria-modal="true"]')) return;
            if (event.key === 'Escape') {
              event.preventDefault();
              event.stopPropagation();
              onSettingsOpenChange(false);
              settingsButtonRef.current?.focus();
            }
          }} style={{ left: `calc(${panelOffset.left}px + var(--sidebar-width, 0px) + 10px)`, bottom: panelOffset.bottom }}
          className="absolute w-[300px] p-[14px] bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40 max-h-[calc(100vh-80px)] overflow-y-auto">
            <div className="ui-label text-[var(--ink-3)] mb-2.5">
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
                  className={`${segBase} flex-1 !h-[24px] !px-1 text-caption ${activeSettingsTab === key ? segActive : segInactive}`}
                >
                  {label}
                </button>
              ))}
            </div>

            <button type="button" aria-label={t('settingsClose')} title={t('settingsClose')}
              onClick={() => { onSettingsOpenChange(false); settingsButtonRef.current?.focus(); }}
              className="absolute top-[10px] right-[10px] w-[22px] h-[22px] grid place-items-center rounded-[3px] text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
              <Icon name="close" size={16} />
            </button>

            {activeSettingsTab === 'general' && (
              <>
                <button role="switch" aria-checked={singleKeyShortcuts} onClick={() => onSingleKeyShortcutsChange?.(!singleKeyShortcuts)} className="w-full text-left border border-[var(--line)] rounded p-2 mb-2 cursor-pointer">
                  <span aria-hidden="true">{singleKeyShortcuts ? <Icon name="check" size={14} /> : '○ '}</span>{t('singleKeyShortcutsTitle')}
                </button>
                <p className="text-small text-[var(--ink-2)] mb-4">{t('singleKeyShortcutsDescription')}</p>
                <div className="mb-4 p-2 border border-[var(--line)] rounded text-small">
                  <p>{t('settingsPrintersMovedHint')}</p>
                  <button className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] mt-2 text-[var(--accent)] focus-visible:outline-2" onClick={() => {
                    onSettingsOpenChange(false); onMainViewChange('printers');
                  }}>{t('pmOpen')}</button>
                </div>
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
                <div className="mt-2 font-medium tabular-nums text-caption leading-relaxed text-[var(--ink-3)]">
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
                <div className="mt-2 font-medium tabular-nums text-caption leading-relaxed text-[var(--ink-3)]">
                  {uiDensity === 'compact' ? t('densityDescriptionCompact') : t('densityDescriptionComfort')}
                </div>

                {onDetailPanelChange && <fieldset className="mt-3"><legend>{t('detailPanelLabel')}</legend>
                  {(['auto', 'pinned'] as const).map(mode => <label key={mode} className="flex items-center gap-2 mt-1">
                    <input type="radio" name="detailPanel" checked={detailPanel === mode} onChange={() => onDetailPanelChange(mode)} />
                    {t(mode === 'auto' ? 'detailPanelAuto' : 'detailPanelPinned')}
                  </label>)}
                </fieldset>}
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
                <div className="mt-2 font-medium tabular-nums text-caption leading-relaxed text-[var(--ink-3)]">
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
                  <div className="font-medium tabular-nums text-caption text-[var(--ink-3)]">
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
                            className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-none w-3.5 h-3.5 rounded-full border cursor-pointer grid place-items-center"
                            style={{ borderColor: isPrimary ? 'var(--accent)' : 'var(--line-strong)' }}
                          >
                            {isPrimary && <span className="w-1.5 h-1.5 rounded-full" style={{ background: 'var(--accent)' }} />}
                          </button>
                          <div className="flex-1 min-w-0">
                            <div className="text-[length:var(--font-size-title)] text-[var(--ink)] truncate">{s.name}</div>
                            <div className="flex items-center gap-1.5">
                              {isPrimary && (
                                <span className="flex-none text-compact-label ui-label px-1.5 rounded-full" style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}>
                                  {t('slicerPrimaryChip')}
                                </span>
                              )}
                              <div className="flex-1 min-w-0 font-code text-[var(--ink-3)] truncate">
                                {s.path}
                              </div>
                            </div>
                          </div>
                          <span
                            onClick={() => onRemoveSlicer(s.id)}
                            aria-label={t('removeSlicerAria')}
                            className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                          >
                            <Icon name="close" size={14} />
                          </span>
                        </div>
                      );
                    })}
                  </div>
                )}
                <button
                  onClick={onAddSlicer}
                  className="mt-2 h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                >
                  <Icon name="plus" size={14} /> {t('addSlicer')}
                </button>
                {addSlicerError && (
                  <div className="mt-1.5 font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--accent)] break-words">
                    {t('addSlicerError')} <ErrorText error={addSlicerError} />
                  </div>
                )}
              </>
            )}

            {activeSettingsTab === 'catalog' && (
              <>
                <div className="text-[length:var(--font-size-body)] font-semibold mb-2">{t('catalogCleanupTitle')}</div>
                <button
                  onClick={onScanCatalogIssues}
                  disabled={cleanupScanning} {...lockProps}
                  className={`h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small ${
                    cleanupScanning ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]'
                  }`}
                >
                  {cleanupScanning ? t('catalogCleanupScanning') : t('catalogCleanupScanButton')}
                </button>
                {cleanupError && (
                  <div className="mt-1.5 font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--accent)] break-words">
                    {t('catalogCleanupError')} <ErrorText error={cleanupError} />
                  </div>
                )}

                <div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('catalogBackupTitle')}</div>
                <button
                  onClick={onExportCatalog}
                  className="h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                >
                  {t('exportCatalogButton')}
                </button>
                {confirmImportCatalog ? (
                  <div className="mt-1.5 flex flex-col gap-1.5">
                    <div className="font-medium tabular-nums text-caption text-[var(--ink-2)]">
                      {t('importCatalogConfirmQuestion')}
                    </div>
                    <div className="flex gap-1.5">
                      <button {...lockProps}
                        onClick={() => {
                          setConfirmImportCatalog(false);
                          onImportCatalog();
                        }}
                        className="flex-1 h-7 rounded-[3px] border border-red-400 bg-transparent text-red-400 text-small cursor-pointer hover:bg-red-400/10"
                      >
                        {t('importCatalogConfirmYes')}
                      </button>
                      <button
                        onClick={() => setConfirmImportCatalog(false)}
                        className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)]"
                      >
                        {t('cancel')}
                      </button>
                    </div>
                  </div>
                ) : (
                  <button {...lockProps}
                    onClick={() => { onClearCatalogBackupError?.(); setConfirmImportCatalog(true); }}
                    className="mt-1.5 h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                  >
                    {t('importCatalogButton')}
                  </button>
                )}
                {catalogBackupError && (
                  <div className="mt-1.5 font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--accent)] break-words">
                    <ErrorText error={catalogBackupError} />
                  </div>
                )}

                <CatalogResetSection modelCount={catalogModelCount} folderCount={catalogFolderCount}
                  onExport={onExportCatalog} onReset={onCatalogReset} backupError={catalogBackupError} />
                <div className="text-[length:var(--font-size-body)] font-semibold mt-4 mb-2">{t('catalogBaseDirSectionTitle')}</div>
                <div className={`${catalogBaseDir ? 'font-code' : 'text-caption'} text-[var(--ink-3)] truncate mb-1.5`}>
                  {catalogBaseDir ?? t('catalogBaseDirNotSet')}
                </div>
                <div className="flex gap-1.5">
                  <button
                    onClick={onOpenCatalogSetup}
                    className="flex-1 h-7 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                  >
                    {catalogBaseDir ? t('catalogBaseDirChangeButton') : t('catalogBaseDirSetupButton')}
                  </button>
                  {catalogBaseDir && (
                    <button
                      onClick={() => invoke('open_in_file_manager', { path: catalogBaseDir })}
                      className="flex-1 h-7 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                    >
                      {t('catalogBaseDirOpenButton')}
                    </button>
                  )}
                </div>
              </>
            )}


            {activeSettingsTab === 'info' && (
              <>
                <button onClick={onOpenTips} className="w-full h-8 border border-[var(--line-strong)] rounded cursor-pointer mb-3">{t('keyboardTipsTitle')}</button>
                <UpdatePanel view={update} />
                <DiagnosticsSettings />
                <InfoFolders />
                <div className="flex justify-between text-caption py-2 border-t border-[var(--line)] mt-3 text-[var(--ink-2)]">
                  <span>{t('infoSourceCodeLabel')}</span>
                  <span
                    onClick={() => invoke('open_release_url', { url: 'https://github.com/Bexxs75/3mf-katalog-manager/' }).catch(() => {})}
                    className="cursor-pointer hover:text-[var(--accent)]"
                  >
                    GitHub
                  </span>
                </div>
                <div className="flex justify-between items-center text-caption py-2 border-t border-[var(--line)] text-[var(--ink-2)]">
                  <span>{t('infoCommunityLabel')}</span>
                  <span
                    onClick={() => invoke('open_discord_invite').catch(() => {})}
                    className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex items-center gap-1 cursor-pointer font-semibold"
                    style={{ color: '#5865F2' }}
                  >
                    <svg width="13" height="13" viewBox="0 0 127.14 96.36" fill="currentColor">
                      <path d="M107.7,8.07A105.15,105.15,0,0,0,81.47,0a72.06,72.06,0,0,0-3.36,6.83A97.68,97.68,0,0,0,49,6.83,72.37,72.37,0,0,0,45.64,0,105.89,105.89,0,0,0,19.39,8.09C2.79,32.65-1.71,56.6.54,80.21h0A105.73,105.73,0,0,0,32.71,96.36,77.7,77.7,0,0,0,39.6,85.25a68.42,68.42,0,0,1-10.85-5.18c.91-.66,1.8-1.34,2.66-2a75.57,75.57,0,0,0,64.32,0c.87.71,1.76,1.39,2.66,2a68.68,68.68,0,0,1-10.87,5.19,77,77,0,0,0,6.89,11.1A105.25,105.25,0,0,0,126.6,80.22h0C129.24,52.84,122.09,29.11,107.7,8.07ZM42.45,65.69C36.18,65.69,31,60,31,53s5-12.74,11.43-12.74S54,46,53.89,53,48.84,65.69,42.45,65.69Zm42.24,0C78.41,65.69,73.25,60,73.25,53s5-12.74,11.44-12.74S96.23,46,96.12,53,91.08,65.69,84.69,65.69Z" />
                    </svg>
                    Discord
                  </span>
                </div>
                <div className="flex justify-between text-caption py-1 border-t border-[var(--line)] text-[var(--ink-2)]">
                  <span>{t('infoLicenseLabel')}</span>
                  <span>MIT</span>
                </div>
                <div className="flex justify-between text-caption py-1 text-[var(--ink-2)]">
                  <span>{t('infoThirdPartyLicensesLabel')}</span>
                  {/* Stacked instead of side by side so label and values don't wrap. */}
                  <span className="flex flex-col items-end">
                    {[
                      { name: 'Open CASCADE', anchor: '' },
                      { name: 'UnRAR', anchor: '#unrar' },
                    ].map(({ name, anchor }) => (
                      <span
                        key={name}
                        onClick={() => invoke('open_release_url', { url: `https://github.com/Bexxs75/3mf-katalog-manager/blob/master/THIRD-PARTY-LICENSES.md${anchor}` }).catch(() => {})}
                        className="cursor-pointer hover:text-[var(--accent)]"
                      >
                        {name}
                      </span>
                    ))}
                  </span>
                </div>
              </>
            )}
          </div>
        )}
      </div>
    </nav>
  );
}
