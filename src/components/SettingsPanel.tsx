import { displayName } from '../lib/displayName';
import { ImportRenameSetting } from './ImportRenameSetting';
import { SourceFoldersSettings } from './SourceFoldersSettings';
import { DiskSyncSettings } from './DiskSyncSettings';
import { AutoTaggingSetting } from './AutoTaggingSetting';
import { ExternalLink } from './ExternalLink';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { useImportLock } from '../hooks/ImportLockContext';
import { Icon } from './Icon';
import { CatalogResetSection } from './CatalogResetSection';
import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { invoke } from '../lib/transport';
import type { SlicerConfig } from '../types';
import type { ThemeSetting } from '../hooks/useTheme';
import type { UiDensity } from '../hooks/UiDensityContext';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import type { Language } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { DiagnosticsSettings } from '../diagnostics/DiagnosticsSettings';
import { LinuxMenuSettings } from './LinuxMenuSettings';
import { InfoFolders } from '../diagnostics/InfoFolders';
import type { AppError } from '../lib/errors';
import { WebCatalogBackup } from './WebCatalogBackup';
import { ErrorText } from '../diagnostics/ErrorText';
import type { UpdaterView } from '../hooks/useUpdater';
import { UpdatePanel } from './UpdatePanel';
import { getAppVersion } from '../lib/api/update';

type SettingsTab = 'general' | 'slicer' | 'catalog' | 'info';
const TABS: readonly SettingsTab[] = import.meta.env.VITE_TARGET === 'web' ? ['general', 'catalog', 'info'] : ['general', 'slicer', 'catalog', 'info'];

export interface SettingsPanelProps {
  /** Lay the panel over the main area instead of taking the detail panel's place in the layout. */
  overlay: boolean;
  onClose: () => void;
  /** Leaves the settings and shows the Printer Manager. */
  onOpenPrinters: () => void;
  detailPanel?: 'auto' | 'pinned';
  onDetailPanelChange?: (mode: 'auto' | 'pinned') => void;
  singleKeyShortcuts?: boolean;
  onSingleKeyShortcutsChange?: (enabled: boolean) => void;
  onOpenTips?: () => void;
  /** Changes whenever the app wants the General tab (for example to point at the source folders). */
  generalTabRequest?: number;
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

export function SettingsPanel({
  overlay, onClose, onOpenPrinters,
  detailPanel = 'auto', onDetailPanelChange,
  singleKeyShortcuts = true, onSingleKeyShortcutsChange, onOpenTips,
  generalTabRequest,
  themeSetting, onThemeChange,
  uiDensity, onUiDensityChange,
  displayPreference, onDisplayPreferenceChange,
  slicers, primarySlicerId, onAddSlicer, addSlicerError, onRemoveSlicer, onSetPrimarySlicer,
  onScanCatalogIssues, cleanupScanning, cleanupError,
  onExportCatalog, catalogModelCount, catalogFolderCount, onCatalogReset, onImportCatalog,
  onClearCatalogBackupError, catalogBackupError, catalogBaseDir, onOpenCatalogSetup,
  update,
}: SettingsPanelProps) {
  const { lockProps } = useImportLock();
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const { language, setLanguage } = useLanguage();
  const [confirmImportCatalog, setConfirmImportCatalog] = useState(false);
  const [activeSettingsTab, setActiveSettingsTab] = useState<SettingsTab>('general');
  const panelRef = useRef<HTMLElement>(null);
  const tabRefs = useRef<Partial<Record<SettingsTab, HTMLButtonElement | null>>>({});

  useEffect(() => { if (generalTabRequest) setActiveSettingsTab('general'); }, [generalTabRequest]);

  // Move focus into the panel when it opens so keyboard and screen reader users land in it.
  useEffect(() => { tabRefs.current.general?.focus(); }, []);

  // Escape closes the panel from inside it, or when nothing else holds focus. A dialog or an
  // open menu handles Escape itself, so leave it alone then.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented) return;
      if (document.querySelector('[role="dialog"], [role="menu"], [role="listbox"], [data-navigation-menu], .menu')) return;
      const active = document.activeElement;
      if (active && active !== document.body && !panelRef.current?.contains(active)) return;
      event.preventDefault();
      onClose();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  const onTabKeyDown = (event: ReactKeyboardEvent, index: number) => {
    const target = event.key === 'ArrowRight' ? (index + 1) % TABS.length
      : event.key === 'ArrowLeft' ? (index + TABS.length - 1) % TABS.length
      : event.key === 'Home' ? 0
      : event.key === 'End' ? TABS.length - 1 : null;
    if (target === null) return;
    event.preventDefault();
    setActiveSettingsTab(TABS[target]);
    tabRefs.current[TABS[target]]?.focus();
  };

  const tabLabels: Record<SettingsTab, string> = {
    general: t('settingsTabGeneral'), slicer: t('settingsTabSlicer'),
    catalog: t('settingsTabCatalog'), info: t('settingsTabInfo'),
  };

  return (
    <div role="complementary" aria-label={t('settingsTitle')} className={`detail-panel-shell ${overlay ? 'detail-panel-overlay' : ''}`}>
      <aside ref={panelRef} className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
        <div className="flex-none flex items-center justify-between px-4 pt-3.5 pb-2.5">
          <h2 className="ui-label text-[var(--ink-3)]">{t('settingsTitle')}</h2>
          <button type="button" aria-label={t('settingsClose')} title={`${t('settingsClose')} (Esc)`} onClick={onClose}
            className="w-[26px] h-[26px] grid place-items-center rounded-[3px] text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
            <Icon name="close" size={16} />
          </button>
        </div>
        <div role="tablist" aria-label={t('settingsTitle')} className="flex-none flex p-0.5 gap-0.5 mx-4 mb-3 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
          {TABS.map((key, index) => (
            <button
              key={key}
              ref={node => { tabRefs.current[key] = node; }}
              type="button" role="tab" id={`settings-tab-${key}`} aria-controls="settings-tabpanel"
              aria-selected={activeSettingsTab === key} tabIndex={activeSettingsTab === key ? 0 : -1}
              onClick={() => setActiveSettingsTab(key)}
              onKeyDown={event => onTabKeyDown(event, index)}
              className={`${segBase} flex-1 !h-[24px] !px-1 text-caption focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${activeSettingsTab === key ? segActive : segInactive}`}
            >
              {tabLabels[key]}
            </button>
          ))}
        </div>
        <div role="tabpanel" id="settings-tabpanel" aria-labelledby={`settings-tab-${activeSettingsTab}`}
          className="flex-1 min-h-0 overflow-y-auto scrollbar-stable px-4 pb-4">
          {activeSettingsTab === 'general' && (
            <>
              {import.meta.env.VITE_TARGET !== 'web' && <>
              <button role="switch" aria-checked={singleKeyShortcuts} onClick={() => onSingleKeyShortcutsChange?.(!singleKeyShortcuts)} className="w-full text-left border border-[var(--line)] rounded p-2 mb-2 cursor-pointer">
                <span aria-hidden="true">{singleKeyShortcuts ? <Icon name="check" size={14} /> : '○ '}</span>{t('singleKeyShortcutsTitle')}
              </button>
              <p className="text-small text-[var(--ink-2)] mb-4">{t('singleKeyShortcutsDescription')}</p>
              </>}
              {import.meta.env.VITE_TARGET !== 'web' && <>
                <ImportRenameSetting />
                <DiskSyncSettings />
                <SourceFoldersSettings />
              </>}
              <div className="mb-4 p-2 border border-[var(--line)] rounded text-small">
                <p>{t('settingsPrintersMovedHint')}</p>
                <button className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] mt-2 text-[var(--accent)] focus-visible:outline-2" onClick={() => {
                  onOpenPrinters();
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
              {import.meta.env.VITE_TARGET !== 'web' && <LinuxMenuSettings />}
            </>
          )}

          {import.meta.env.VITE_TARGET !== 'web' && activeSettingsTab === 'slicer' && container && <p className="text-small text-[var(--ink-3)]">{t('containerSlicerUnavailable')}</p>}
          {import.meta.env.VITE_TARGET !== 'web' && activeSettingsTab === 'slicer' && !container && (
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
                              {displayName(s.path)}
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

          {/* The web catalog tab holds the switch, the block list and the backup download: cleanup and import need the desktop. */}
          {import.meta.env.VITE_TARGET === 'web' && activeSettingsTab === 'catalog' && (
            <>
              <AutoTaggingSetting />
              <WebCatalogBackup />
            </>
          )}

          {import.meta.env.VITE_TARGET !== 'web' && activeSettingsTab === 'catalog' && (
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

              <AutoTaggingSetting />
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
                {catalogBaseDir ? displayName(catalogBaseDir) : t('catalogBaseDirNotSet')}
              </div>
              <div className="flex gap-1.5">
                <button
                  onClick={onOpenCatalogSetup}
                  className="flex-1 h-7 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                >
                  {catalogBaseDir ? t('catalogBaseDirChangeButton') : t('catalogBaseDirSetupButton')}
                </button>
                {catalogBaseDir && !container && (
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
              {import.meta.env.VITE_TARGET === 'web' && <WebVersionRow />}
              {import.meta.env.VITE_TARGET !== 'web' && <UpdatePanel view={update} onExport={onExportCatalog} exportError={catalogBackupError} />}
              {import.meta.env.VITE_TARGET !== 'web' && <DiagnosticsSettings />}
              {import.meta.env.VITE_TARGET !== 'web' && <InfoFolders />}
              <div className="flex justify-between text-caption py-2 border-t border-[var(--line)] mt-3 text-[var(--ink-2)]">
                <span>{t('infoSourceCodeLabel')}</span>
                <ExternalLink url="https://github.com/Bexxs75/3mf-katalog-manager/" label="GitHub"
                  onOpen={() => { void invoke('open_release_url', { url: 'https://github.com/Bexxs75/3mf-katalog-manager/' }).catch(() => {}); }}
                  className="cursor-pointer hover:text-[var(--accent)]"
                >
                  GitHub
                </ExternalLink>
              </div>
              <div className="flex justify-between items-center text-caption py-2 border-t border-[var(--line)] text-[var(--ink-2)]">
                <span>{t('infoCommunityLabel')}</span>
                <ExternalLink url="https://discord.gg/abfVNfFqu3" label="Discord"
                  onOpen={() => { void invoke('open_discord_invite').catch(() => {}); }}
                  className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex items-center gap-1 cursor-pointer font-semibold"
                  style={{ color: '#5865F2' }}
                >
                  <svg width="13" height="13" viewBox="0 0 127.14 96.36" fill="currentColor">
                    <path d="M107.7,8.07A105.15,105.15,0,0,0,81.47,0a72.06,72.06,0,0,0-3.36,6.83A97.68,97.68,0,0,0,49,6.83,72.37,72.37,0,0,0,45.64,0,105.89,105.89,0,0,0,19.39,8.09C2.79,32.65-1.71,56.6.54,80.21h0A105.73,105.73,0,0,0,32.71,96.36,77.7,77.7,0,0,0,39.6,85.25a68.42,68.42,0,0,1-10.85-5.18c.91-.66,1.8-1.34,2.66-2a75.57,75.57,0,0,0,64.32,0c.87.71,1.76,1.39,2.66,2a68.68,68.68,0,0,1-10.87,5.19,77,77,0,0,0,6.89,11.1A105.25,105.25,0,0,0,126.6,80.22h0C129.24,52.84,122.09,29.11,107.7,8.07ZM42.45,65.69C36.18,65.69,31,60,31,53s5-12.74,11.43-12.74S54,46,53.89,53,48.84,65.69,42.45,65.69Zm42.24,0C78.41,65.69,73.25,60,73.25,53s5-12.74,11.44-12.74S96.23,46,96.12,53,91.08,65.69,84.69,65.69Z" />
                  </svg>
                  Discord
                </ExternalLink>
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
                    <ExternalLink
                      key={name} label={name}
                      url={`https://github.com/Bexxs75/3mf-katalog-manager/blob/master/THIRD-PARTY-LICENSES.md${anchor}`}
                      onOpen={() => { void invoke('open_release_url', { url: `https://github.com/Bexxs75/3mf-katalog-manager/blob/master/THIRD-PARTY-LICENSES.md${anchor}` }).catch(() => {}); }}
                      className="cursor-pointer hover:text-[var(--accent)]"
                    >
                      {name}
                    </ExternalLink>
                  ))}
                </span>
              </div>
            </>
          )}
        </div>
      </aside>
    </div>
  );
}

/** The web build has no updater panel (which shows the version on the desktop), so the version gets its own row. */
function WebVersionRow() {
  const t = useT();
  const [version, setVersion] = useState('');
  useEffect(() => {
    let alive = true;
    getAppVersion().then(v => { if (alive) setVersion(v); }).catch(() => {});
    return () => { alive = false; };
  }, []);
  if (!version) return null;
  return (
    <div className="flex justify-between text-caption py-2 border-t border-[var(--line)] mt-3 text-[var(--ink-2)]">
      <span>{t('infoVersionLabel')}</span>
      <span data-testid="web-version">{version}</span>
    </div>
  );
}
