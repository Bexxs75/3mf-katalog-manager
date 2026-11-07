import { PrintTechnologySettings } from './PrintTechnologySettings';
import { SlicerReportSettings } from './SlicerReportSettings';
import { isReadOnly } from '../lib/readOnly';
import { displayName } from '../lib/displayName';
import { ImportRenameSetting } from './ImportRenameSetting';
import { SourceFoldersSettings } from './SourceFoldersSettings';
import { DiskSyncSettings } from './DiskSyncSettings';
import { AutoTaggingSetting } from './AutoTaggingSetting';
import { ExternalLink } from './ExternalLink';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { useSettingsLayout } from '../hooks/useSettingsLayout';
import { useImportLock } from '../hooks/ImportLockContext';
import { Icon } from './Icon';
import { CatalogResetSection } from './CatalogResetSection';
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { invoke } from '../lib/transport';
import type { SlicerConfig } from '../types';
import type { ThemeSetting } from '../hooks/useTheme';
import type { UiDensity } from '../hooks/UiDensityContext';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import type { Language } from '../i18n/types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { DiagnosticsSettings } from '../diagnostics/DiagnosticsSettings';
import { LinuxMenuSettings } from './LinuxMenuSettings';
import { CloseBehaviorSettings } from './CloseBehaviorSettings';
import { InfoFolders } from '../diagnostics/InfoFolders';
import type { AppError } from '../lib/errors';
import { WebCatalogBackup } from './WebCatalogBackup';
import { ErrorText } from '../diagnostics/ErrorText';
import type { UpdaterView } from '../hooks/useUpdater';
import { UpdatePanel } from './UpdatePanel';
import { PrintDataRereadSettings } from './PrintDataRereadSettings';
import { ThingiverseSettings } from './ThingiverseSettings';
import { DragOutSetting } from './DragOutSetting';
import { RadioSegments } from './RadioSegments';
import { ToggleSwitch } from './ToggleSwitch';
import { getAppVersion } from '../lib/api/update';
import { useLoadedSetting } from './SettingState';
import { SettingsSection } from './SettingsSection';
import { UpdateChannelControl } from './UpdateChannelControl';
import { isUpdateCheckDisabled } from '../hooks/useUpdater';
import { ModalDialogOwner, useModalDialog } from '../hooks/useModalDialog';
import { SETTINGS_TARGETS, settingsHeadingId, type SettingsRequest, type SettingsTab, type SettingsTarget } from '../lib/settingsNav';

const TABS: readonly SettingsTab[] = import.meta.env.VITE_TARGET === 'web'
  ? ['general', 'catalog', 'maintenance', 'info']
  : ['general', 'catalog', 'services', 'maintenance', 'info'];

export interface SettingsPanelProps {
  /** Lay the panel over the main area instead of taking the detail panel's place in the layout. */
  overlay: boolean;
  onClose: () => void;
  /** Leaves the settings and shows the Printer Manager. */
  onOpenPrinters: () => void;
  /** Told when the Thingiverse switch changes, so the rail entry follows at once. */
  onThingiverseEnabledChange?: (enabled: boolean) => void;
  detailPanel?: 'auto' | 'pinned';
  onDetailPanelChange?: (mode: 'auto' | 'pinned') => void;
  singleKeyShortcuts?: boolean;
  onSingleKeyShortcutsChange?: (enabled: boolean) => void;
  onOpenTips?: () => void;
  /** A section the app wants shown (for example the source folders); a new request moves the panel again. */
  request?: SettingsRequest | null;
  /** Tab to open on without a request; the app keeps the last one used for the session. */
  initialTab?: SettingsTab;
  onTabChange?: (tab: SettingsTab) => void;
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
const dashedButton = 'min-h-7 py-1 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]';

const LANGUAGE_LABELS: Record<Language, string> = {
  de: 'Deutsch',
  en: 'English',
  es: 'Español',
  fr: 'Français',
};

export function SettingsPanel({
  overlay: forceOverlay, onClose, onOpenPrinters, onThingiverseEnabledChange,
  detailPanel = 'auto', onDetailPanelChange,
  singleKeyShortcuts = true, onSingleKeyShortcutsChange, onOpenTips,
  request, initialTab, onTabChange,
  themeSetting, onThemeChange,
  uiDensity, onUiDensityChange,
  displayPreference, onDisplayPreferenceChange,
  slicers, primarySlicerId, onAddSlicer, addSlicerError, onRemoveSlicer, onSetPrimarySlicer,
  onScanCatalogIssues, cleanupScanning, cleanupError,
  onExportCatalog, catalogModelCount, catalogFolderCount, onCatalogReset, onImportCatalog,
  onClearCatalogBackupError, catalogBackupError, catalogBaseDir, onOpenCatalogSetup,
  update,
}: SettingsPanelProps) {
  // The view says whether it can dock the panel at all; the width rules decide the rest.
  const layout = useSettingsLayout(forceOverlay);
  const overlay = layout.overlay;
  const { lockProps } = useImportLock();
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const { language, setLanguage } = useLanguage();
  const web = import.meta.env.VITE_TARGET === 'web';
  const [confirmImportCatalog, setConfirmImportCatalog] = useState(false);
  const [activeSettingsTab, setActiveSettingsTab] = useState<SettingsTab>(() => {
    const wanted = request ? SETTINGS_TARGETS[request.target].tab : initialTab;
    return wanted && TABS.includes(wanted) ? wanted : 'general';
  });
  const panelRef = useRef<HTMLElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const tabRefs = useRef<Partial<Record<SettingsTab, HTMLButtonElement | null>>>({});

  // The opener belongs to the panel lifetime, not to its current layout.
  const [trigger] = useState(() => document.activeElement as HTMLElement | null);
  const dialogRef = useModalDialog<HTMLDivElement>({ open: overlay, onClose, restoreFocus: false, initialFocus: '[role="tab"][aria-selected="true"]' });
  useLayoutEffect(() => () => {
    if (trigger?.isConnected) trigger.focus({ preventScroll: true });
  }, [trigger]);
  const [visitedTabs, setVisitedTabs] = useState(() => new Set<SettingsTab>([activeSettingsTab]));

  // After a slicer is removed the focus moves on to the next entry, or to the add button when the last one went.
  const slicerRadioRefs = useRef<Record<string, HTMLInputElement | null>>({});
  const addSlicerRef = useRef<HTMLButtonElement>(null);
  const slicerFocusAfterRemove = useRef<{ removed: string; next: string | null } | null>(null);
  useEffect(() => {
    const pending = slicerFocusAfterRemove.current;
    if (!pending || slicers.some(s => s.id === pending.removed)) return;
    slicerFocusAfterRemove.current = null;
    (pending.next ? slicerRadioRefs.current[pending.next] : addSlicerRef.current)?.focus();
  }, [slicers]);

  const selectTab = (tab: SettingsTab) => { setVisitedTabs(previous => new Set([...previous, tab])); setActiveSettingsTab(tab); onTabChange?.(tab); };

  // The one way to a section: the app's requests and the link inside the panel both go through here.
  const pendingSection = useRef<string | null>(null);
  const [navTick, setNavTick] = useState(0);
  const goTo = (target: SettingsTarget) => {
    const { tab, section } = SETTINGS_TARGETS[target];
    pendingSection.current = section;
    if (TABS.includes(tab)) selectTab(tab);
    setNavTick(n => n + 1);
  };
  const handledRequest = useRef<SettingsRequest | null>(null);
  useEffect(() => {
    if (!request || handledRequest.current === request) return;
    handledRequest.current = request;
    goTo(request.target);
    // goTo only reads refs and stable setters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request]);

  // Every tab starts at its top; a requested section is scrolled to and its heading takes the focus.
  useLayoutEffect(() => { if (bodyRef.current) bodyRef.current.scrollTop = 0; }, [activeSettingsTab]);
  useLayoutEffect(() => {
    const section = pendingSection.current;
    if (!section) return;
    pendingSection.current = null;
    const heading = document.getElementById(settingsHeadingId(section as Parameters<typeof settingsHeadingId>[0]));
    if (!heading) { tabRefs.current[activeSettingsTab]?.focus(); return; }
    heading.focus({ preventScroll: true });
    heading.scrollIntoView?.({ block: 'start' });
  }, [activeSettingsTab, navTick]);

  // Move focus into the panel when it opens so keyboard and screen reader users land in it.
  useEffect(() => { if (!pendingSection.current) tabRefs.current[activeSettingsTab]?.focus(); }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Docked, Escape closes the panel from inside it, or when nothing else holds focus. A dialog or an
  // open menu handles Escape itself, so leave it alone then. As an overlay the modal behavior does it.
  useEffect(() => {
    if (overlay) return;
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
  }, [onClose, overlay]);

  const onTabKeyDown = (event: ReactKeyboardEvent, index: number) => {
    const target = event.key === 'ArrowRight' ? (index + 1) % TABS.length
      : event.key === 'ArrowLeft' ? (index + TABS.length - 1) % TABS.length
      : event.key === 'Home' ? 0
      : event.key === 'End' ? TABS.length - 1 : null;
    if (target === null) return;
    event.preventDefault();
    selectTab(TABS[target]);
    tabRefs.current[TABS[target]]?.focus();
  };

  const tabLabels: Record<SettingsTab, string> = {
    general: t('settingsTabGeneral'), catalog: t('settingsTabCatalog'), services: t('settingsTabServices'),
    maintenance: t('settingsTabMaintenance'), info: t('settingsTabInfo'),
  };

  const printersSection = (
    <SettingsSection id="printers" title={t('settingsSecPrinters')}>
      <p className="text-small text-[var(--ink-2)]">{t('settingsPrintersMovedHint')}</p>
      <button className="mt-2 hover:bg-[var(--panel-2)] hover:text-[var(--ink)] text-[var(--accent)] focus-visible:outline-2" onClick={onOpenPrinters}>{t('pmOpen')}</button>
    </SettingsSection>
  );

  return (
    <ModalDialogOwner.Provider value={dialogRef}>
    <div ref={dialogRef} role={overlay ? 'dialog' : 'complementary'} aria-modal={overlay ? true : undefined} aria-label={t('settingsTitle')}
      style={{ '--settings-panel-width': layout.width === null ? '100%' : `${layout.width}px`, '--settings-pad': `${layout.pad}px` } as CSSProperties}
      className={`detail-panel-shell settings-panel ${overlay ? 'settings-panel-overlay' : ''}`}>
      {overlay && <div aria-hidden="true" className="settings-panel-scrim" />}
      <aside ref={panelRef} className="settings-panel-aside flex-none flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
        <div className="flex-none flex items-center justify-between settings-panel-pad-x pt-3.5 pb-2.5">
          <h2 className="text-[length:var(--font-size-title)] font-semibold text-[var(--ink)]">{t('settingsTitle')}</h2>
          <button type="button" aria-label={t('settingsClose')} title={`${t('settingsClose')} (Esc)`} onClick={onClose}
            className="w-[26px] h-[26px] grid place-items-center rounded-[3px] text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
            <Icon name="close" size={16} />
          </button>
        </div>
        <div role="tablist" aria-label={t('settingsTabsLabel')} className="flex-none flex flex-wrap p-0.5 gap-0.5 settings-panel-margin-x mb-3 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
          {TABS.map((key, index) => (
            <button
              key={key}
              ref={node => { tabRefs.current[key] = node; }}
              type="button" role="tab" id={`settings-tab-${key}`} aria-controls="settings-tabpanel"
              aria-selected={activeSettingsTab === key} tabIndex={activeSettingsTab === key ? 0 : -1}
              onClick={() => selectTab(key)}
              onKeyDown={event => onTabKeyDown(event, index)}
              className={`${segBase} flex-auto whitespace-nowrap !h-auto min-h-[26px] [@media(pointer:coarse)]:min-h-[44px] !px-2 !py-1 text-caption focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${activeSettingsTab === key ? segActive : segInactive}`}
            >
              {tabLabels[key]}
            </button>
          ))}
        </div>
        <div ref={bodyRef} role="tabpanel" id="settings-tabpanel" aria-labelledby={`settings-tab-${activeSettingsTab}`}
          className="flex-1 min-h-0 overflow-y-auto scrollbar-stable settings-panel-pad-x pb-4">
          {visitedTabs.has('general') && (
            <div hidden={activeSettingsTab !== 'general'}>
              <SettingsSection id="appearance" title={t('settingsSecAppearance')}>
                <RadioSegments
                  legend={t('themeLegend')}
                  options={(['system', 'light', 'dark'] as ThemeSetting[]).map(opt => ({
                    value: opt,
                    label: opt === 'system' ? t('themeSystem') : opt === 'light' ? t('themeLight') : t('themeDark'),
                  }))}
                  value={themeSetting}
                  onChange={onThemeChange}
                />
                <div className="mt-2 font-medium tabular-nums text-caption leading-relaxed text-[var(--ink-3)]">
                  {themeSetting === 'system'
                    ? t('themeDescriptionSystem')
                    : t('themeDescriptionManual').replace(
                        '{mode}',
                        themeSetting === 'light' ? t('themeLight') : t('themeDark'),
                      )}
                </div>

                <RadioSegments
                  className="mt-4"
                  legend={t('densityLegend')}
                  options={(['compact', 'comfort'] as UiDensity[]).map(opt => ({
                    value: opt, label: opt === 'compact' ? t('densityCompact') : t('densityComfort'),
                  }))}
                  value={uiDensity}
                  onChange={onUiDensityChange}
                />
                <div className="mt-2 font-medium tabular-nums text-caption leading-relaxed text-[var(--ink-3)]">
                  {uiDensity === 'compact' ? t('densityDescriptionCompact') : t('densityDescriptionComfort')}
                </div>

                <RadioSegments
                  className="mt-4"
                  legend={t('displayPreferenceTitle')}
                  options={(['thumbnail', 'render'] as DisplayPreference[]).map(opt => ({
                    value: opt, label: opt === 'thumbnail' ? t('displayPreferenceThumbnail') : t('displayPreferenceRender'),
                  }))}
                  value={displayPreference}
                  onChange={onDisplayPreferenceChange}
                />
                <div className="mt-2 font-medium tabular-nums text-caption leading-relaxed text-[var(--ink-3)]">
                  {displayPreference === 'thumbnail'
                    ? t('displayPreferenceDescriptionThumbnail')
                    : t('displayPreferenceDescriptionRender')}
                </div>

                {onDetailPanelChange && <RadioSegments
                  className="mt-4"
                  legend={t('detailPanelLabel')}
                  options={(['auto', 'pinned'] as const).map(mode => ({
                    value: mode, label: t(mode === 'auto' ? 'detailPanelAuto' : 'detailPanelPinned'),
                  }))}
                  value={detailPanel}
                  onChange={onDetailPanelChange}
                />}
                {onDetailPanelChange && <div className="mt-2 font-medium tabular-nums text-caption leading-relaxed text-[var(--ink-3)]">{t('detailPanelHint')}</div>}
              </SettingsSection>

              <SettingsSection id="language" title={t('settingsSecLanguage')}>
                <RadioSegments
                  grid
                  hideLegend
                  legend={t('languageTitle')}
                  options={(['de', 'en', 'es', 'fr'] as Language[]).map(lang => ({ value: lang, label: LANGUAGE_LABELS[lang] }))}
                  value={language}
                  onChange={setLanguage}
                />
              </SettingsSection>

              <SettingsSection id="controls" title={t('settingsSecControls')}>
                {!web && <>
                  <div className="flex items-center justify-between gap-2 text-small">
                    <span className="font-semibold">{t('singleKeyShortcutsTitle')}</span>
                    <ToggleSwitch checked={singleKeyShortcuts} label={t('singleKeyShortcutsTitle')} aria-describedby="single-key-shortcuts-desc"
                      onChange={next => onSingleKeyShortcutsChange?.(next)} />
                  </div>
                  <p id="single-key-shortcuts-desc" className="mt-1 text-small text-[var(--ink-2)] mb-3">{t('singleKeyShortcutsDescription')}</p>
                </>}
                <button onClick={onOpenTips} className="w-full min-h-8 py-1 border border-[var(--line-strong)] rounded cursor-pointer">{t('keyboardTipsTitle')}</button>
              </SettingsSection>

              <SettingsSection id="printMethod" title={t('settingsSecPrintMethod')}>
                <PrintTechnologySettings />
              </SettingsSection>

              {web && printersSection}

              {!web && (
                <SettingsSection id="program" title={t('settingsSecProgram')} hideWhenEmpty>
                  {!container && <CloseBehaviorSettings />}
                  <LinuxMenuSettings />
                </SettingsSection>
              )}
            </div>
          )}

          {visitedTabs.has('catalog') && (
            <div hidden={activeSettingsTab !== 'catalog'}>
              {!web && (
                <>
                  <SettingsSection id="storage" title={t('settingsSecStorage')}>
                    {catalogBaseDir && container
                      ? <div className="text-caption text-[var(--ink-3)] settings-path mb-1.5">{t('containerCatalogPath').replace('{path}', displayName(catalogBaseDir))}</div>
                      : <div className={`${catalogBaseDir ? 'font-code' : 'text-caption'} text-[var(--ink-3)] settings-path mb-1.5`}>
                        {catalogBaseDir ? displayName(catalogBaseDir) : t('catalogBaseDirNotSet')}
                      </div>}
                    <div className="flex flex-wrap gap-1.5">
                      <button onClick={onOpenCatalogSetup} className={`flex-1 min-w-[10rem] ${dashedButton}`}>
                        {catalogBaseDir ? t('catalogBaseDirChangeButton') : t('catalogBaseDirSetupButton')}
                      </button>
                      {catalogBaseDir && !container && (
                        <button onClick={() => invoke('open_in_file_manager', { path: catalogBaseDir })} className={`flex-1 min-w-[10rem] ${dashedButton}`}>
                          {t('catalogBaseDirOpenButton')}
                        </button>
                      )}
                    </div>
                    {!container && (
                      <button type="button" onClick={() => goTo('dragOut')}
                        className="mt-2 text-small text-[var(--accent)] cursor-pointer hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
                        {t('settingsGoToHandover')}
                      </button>
                    )}
                  </SettingsSection>
                  <SettingsSection id="sources" title={t('settingsSecSources')}><SourceFoldersSettings /></SettingsSection>
                  <SettingsSection id="import" title={t('settingsSecImport')}><ImportRenameSetting /></SettingsSection>
                  <SettingsSection id="sync" title={t('settingsSecSync')}><DiskSyncSettings /></SettingsSection>
                </>
              )}
              <SettingsSection id="autoTags" title={t('settingsSecAutoTags')}><AutoTaggingSetting /></SettingsSection>
              <SettingsSection id="printData" title={t('settingsSecPrintData')}><PrintDataRereadSettings /></SettingsSection>
            </div>
          )}

          {!web && visitedTabs.has('services') && (
            <div hidden={activeSettingsTab !== 'services'}>
              {container && <p className="mb-2 p-2 rounded border border-[var(--line)] bg-[var(--panel-2)] text-small text-[var(--ink-2)]">{t('containerSlicerUnavailable')}</p>}
              {!container && (
                <>
                  <SettingsSection id="handover" title={t('settingsSecHandover')} experimental><DragOutSetting /></SettingsSection>
                  <SettingsSection id="slicers" title={t('settingsSecSlicers')}>
                    {slicers.length === 0 ? (
                      <div className="font-medium tabular-nums text-caption text-[var(--ink-3)]">
                        {t('noSlicersConfigured')}
                      </div>
                    ) : (
                      <fieldset className="min-w-0 border-0 p-0 m-0">
                        <legend className="sr-only">{t('slicerDefaultLegend')}</legend>
                        <div className="flex flex-col gap-1.5">
                        {slicers.map((s, index) => {
                          const isPrimary = s.id === primarySlicerId;
                          return (
                            <div key={s.id} className="flex items-center gap-1">
                              <label className="flex-none grid place-items-center w-8 h-8 [@media(pointer:coarse)]:w-11 [@media(pointer:coarse)]:h-11 cursor-pointer">
                                <input
                                  type="radio" name="primary-slicer" aria-label={s.name}
                                  ref={node => { slicerRadioRefs.current[s.id] = node; }}
                                  checked={isPrimary} onChange={() => onSetPrimarySlicer(s.id)}
                                  className="w-4 h-4 m-0 cursor-pointer accent-[var(--accent)]"
                                />
                              </label>
                              <div className="flex-1 min-w-0">
                                <div className="text-[length:var(--font-size-title)] text-[var(--ink)] [overflow-wrap:anywhere]">{s.name}</div>
                                <div className="flex items-center gap-1.5">
                                  {isPrimary && (
                                    <span className="flex-none text-compact-label ui-label px-1.5 rounded-full" style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}>
                                      {t('slicerPrimaryChip')}
                                    </span>
                                  )}
                                  <div className="flex-1 min-w-0 font-code text-[var(--ink-3)] settings-path">
                                    {displayName(s.path)}
                                  </div>
                                </div>
                              </div>
                              <button
                                type="button"
                                onClick={() => {
                                  slicerFocusAfterRemove.current = { removed: s.id, next: slicers[index + 1]?.id ?? null };
                                  onRemoveSlicer(s.id);
                                }}
                                aria-label={t('removeSlicerAria').replace('{name}', s.name)}
                                className="flex-none w-8 h-8 [@media(pointer:coarse)]:w-11 [@media(pointer:coarse)]:h-11 grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
                              >
                                <Icon name="close" size={14} />
                              </button>
                            </div>
                          );
                        })}
                        </div>
                      </fieldset>
                    )}
                    <button ref={addSlicerRef} onClick={onAddSlicer} className={`mt-2 ${dashedButton}`}>
                      {t('addSlicer')}
                    </button>
                    {addSlicerError && (
                      <div className="mt-1.5 font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--accent)] break-words">
                        {t('addSlicerError')} <ErrorText error={addSlicerError} />
                      </div>
                    )}
                  </SettingsSection>
                  <SettingsSection id="slicerReports" title={t('settingsSecSlicerReports')}><SlicerReportSettings active={activeSettingsTab === 'services'} /></SettingsSection>
                </>
              )}
              {printersSection}
              <SettingsSection id="thingiverse" title={t('settingsSecThingiverse')} experimental>
                <ThingiverseSettings onEnabledChange={enabled => onThingiverseEnabledChange?.(enabled)} />
              </SettingsSection>
            </div>
          )}

          {visitedTabs.has('maintenance') && <div hidden={activeSettingsTab !== 'maintenance'}>{web ? (
            <SettingsSection id="webBackup" title={t('settingsSecWebBackup')}><WebCatalogBackup /></SettingsSection>
          ) : (
            <>
              <SettingsSection id="check" title={t('settingsSecCheck')}>
                <button
                  onClick={onScanCatalogIssues}
                  disabled={cleanupScanning} {...lockProps}
                  className={`min-h-7 py-1 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small ${
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
              </SettingsSection>

              <SettingsSection id="backup" title={t('settingsSecBackup')}>
                <button onClick={onExportCatalog} className={dashedButton}>
                  {t('exportCatalogButton')}
                </button>
                {confirmImportCatalog ? (
                  <div className="mt-1.5 flex flex-col gap-1.5">
                    <div className="font-medium tabular-nums text-caption text-[var(--ink-2)]">
                      {t('importCatalogConfirmQuestion')}
                    </div>
                    {!isReadOnly() && <div className="text-caption text-[var(--ink-3)]">{t('tvKeptOnRestore')}</div>}
                    <div className="flex flex-wrap gap-1.5">
                      <button {...lockProps}
                        onClick={() => {
                          setConfirmImportCatalog(false);
                          onImportCatalog();
                        }}
                        className="flex-1 min-w-[8rem] min-h-7 py-1 rounded-[3px] border border-[var(--crit)] bg-transparent text-[var(--crit)] text-small cursor-pointer hover:bg-[var(--panel-2)]"
                      >
                        {t('importCatalogConfirmYes')}
                      </button>
                      <button
                        onClick={() => setConfirmImportCatalog(false)}
                        className="flex-1 min-w-[8rem] min-h-7 py-1 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)]"
                      >
                        {t('cancel')}
                      </button>
                    </div>
                  </div>
                ) : (
                  <button {...lockProps}
                    onClick={() => { onClearCatalogBackupError?.(); setConfirmImportCatalog(true); }}
                    className={`mt-1.5 ${dashedButton}`}
                  >
                    {t('importCatalogButton')}
                  </button>
                )}
                {catalogBackupError && (
                  <div className="mt-1.5 font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--accent)] break-words">
                    <ErrorText error={catalogBackupError} />
                  </div>
                )}
              </SettingsSection>

              <SettingsSection id="reset" title={t('settingsSecReset')} danger>
                <CatalogResetSection modelCount={catalogModelCount} folderCount={catalogFolderCount}
                  onExport={onExportCatalog} onReset={onCatalogReset} backupError={catalogBackupError} />
              </SettingsSection>
            </>
          )}</div>}

          {visitedTabs.has('info') && (
            <div hidden={activeSettingsTab !== 'info'}>
              <SettingsSection id="version" title={t(web ? 'settingsSecVersionWeb' : 'settingsSecVersion')}>
                {web ? <WebVersionRow /> : <UpdatePanel view={update} />}
              </SettingsSection>
              {!web && !container && update.preview === false && (
                <SettingsSection id="preRelease" title={t('settingsSecPreRelease')}>
                  <UpdateChannelControl disabled={isUpdateCheckDisabled(update.phase)} onExport={onExportCatalog} exportError={catalogBackupError} onChanged={update.checkNow} />
                </SettingsSection>
              )}
              {!web && <SettingsSection id="help" title={t('settingsSecHelp')}><DiagnosticsSettings /></SettingsSection>}
              {!web && <SettingsSection id="appData" title={t('settingsSecAppData')}><InfoFolders /></SettingsSection>}
              <SettingsSection id="about" title={t('settingsSecAbout')}>
                <div className="flex justify-between text-caption pb-2 text-[var(--ink-2)]">
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
              </SettingsSection>
            </div>
          )}
        </div>
      </aside>
    </div>
    </ModalDialogOwner.Provider>
  );
}

/** The web build has no updater panel (which shows the version on the desktop), so the version gets its own row. */
function WebVersionRow() {
  const t = useT();
  const { value: version, state, error, reload } = useLoadedSetting(getAppVersion);
  return (
    <div className="text-caption text-[var(--ink-2)]">
      <div className="flex justify-between">
        <span>{t('infoVersionLabel')}</span>
        {version && <span data-testid="web-version">{version}</span>}
      </div>
      {state === 'loading' && <p role="status" className="text-[var(--ink-3)]">{t('settingLoading')}</p>}
      {state === 'failed' && (
        <>
          <p role="alert" className="text-[var(--crit)]">{t('infoVersionLoadFailed')} <ErrorText error={error} /></p>
          <button type="button" onClick={() => void reload()} className="mt-1 min-h-7 py-1 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] font-semibold cursor-pointer hover:border-[var(--accent)]">{t('settingReload')}</button>
        </>
      )}
    </div>
  );
}
