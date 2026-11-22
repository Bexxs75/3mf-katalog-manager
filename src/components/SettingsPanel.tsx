import { SettingsToolsContext, SettingsTabContext, SettingRow, SettingActions, type SettingEntry } from './SettingsTools';
import { SETTING_DEFAULTS, type SettingId } from '../lib/settingsSession';
import { Select } from './Select';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
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
import { CatalogResetSection } from './CatalogResetSection';
import { useCallback, useMemo, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { invoke } from '../lib/transport';
import type { SlicerConfig, ViewMode, SortKey, SortDirection } from '../types';
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
  view?: ViewMode;
  onViewChange?: (value: ViewMode) => void;
  sort?: SortKey;
  onSortChange?: (value: SortKey) => void;
  sortDirection?: SortDirection;
  onSortDirectionChange?: (value: SortDirection) => void;
  collapseBundles?: boolean;
  onCollapseBundlesChange?: (value: boolean) => void;
  detailPanel?: 'auto' | 'pinned';
  onDetailPanelChange?: (mode: 'auto' | 'pinned') => void;
  singleKeyShortcuts?: boolean;
  onSingleKeyShortcutsChange?: (enabled: boolean) => void;
  onOpenTips?: () => void;
  onOpenWhatsNew?: () => void;
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
  view = 'grid', onViewChange, sort = 'name', onSortChange, sortDirection = 'asc', onSortDirectionChange, collapseBundles = true, onCollapseBundlesChange,
  detailPanel = 'auto', onDetailPanelChange,
  singleKeyShortcuts = true, onSingleKeyShortcutsChange, onOpenTips, onOpenWhatsNew,
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
  const themeLabel = (opt: ThemeSetting) => ({
    system: t('themeSystem'), white: t('themeWhite'), light: t('themeSand'), slate: t('themeSlate'), dark: t('themeDark'),
  })[opt];
  const { language, setLanguage } = useLanguage();
  const web = import.meta.env.VITE_TARGET === 'web';
  const [query, setQuery] = useState('');
  const [confirmReset, setConfirmReset] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [entries, setEntries] = useState<Record<string, SettingEntry & { tab: SettingsTab }>>({});
  const [sections, setSections] = useState<Record<string, { tab: SettingsTab; visible: boolean }>>({});
  const report = useCallback((id: SettingId, tab: SettingsTab, entry: SettingEntry | null) => {
    setEntries(previous => { const next = { ...previous }; if (entry) next[id] = { ...entry, tab }; else delete next[id]; return next; });
  }, []);
  const reportSection = useCallback((id: string, tab: SettingsTab, visible: boolean | null) => {
    setSections(previous => {
      if (previous[id]?.visible === visible && previous[id]?.tab === tab) return previous;
      const next = { ...previous }; if (visible === null) delete next[id]; else next[id] = { tab, visible }; return next;
    });
  }, []);
  const tools = useMemo(() => ({ query, report, reportSection }), [query, report, reportSection]);
  const count = Object.values(entries).filter(entry => entry.changed).length;
  const resettable = Object.values(entries).filter(entry => entry.reset && !entry.disabled);
  const resetAll = async () => {
    setResetting(true);
    try { for (const entry of resettable) await entry.reset?.(); }
    finally { setResetting(false); setConfirmReset(false); }
  };
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
  const dialogRef = useModalDialog<HTMLDivElement>({ open: overlay, onClose, backdropSelector: '.settings-panel-scrim', restoreFocus: false, initialFocus: '[role="tab"][aria-selected="true"]' });
  useLayoutEffect(() => () => {
    if (trigger?.isConnected) trigger.focus({ preventScroll: true });
  }, [trigger]);
  const [visitedTabs, setVisitedTabs] = useState(() => new Set<SettingsTab>(TABS));

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
    setQuery('');
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
    heading.closest('section')?.querySelectorAll('details').forEach(details => { details.open = true; });
    heading.focus({ preventScroll: true });
    heading.scrollIntoView?.({ block: 'start' });
    // An opener's context menu may restore focus in its later cleanup. Reclaim
    // only focus outside this panel; never override a user's next control here.
    const focusAfterOpener = setTimeout(() => {
      if (heading.isConnected && !panelRef.current?.contains(document.activeElement)) heading.focus({ preventScroll: true });
    }, 0);
    return () => clearTimeout(focusAfterOpener);
  }, [activeSettingsTab, navTick]);

  // Move focus into the panel when it opens so keyboard and screen reader users land in it.
  useEffect(() => { if (!request && !pendingSection.current) tabRefs.current[activeSettingsTab]?.focus(); }, []); // eslint-disable-line react-hooks/exhaustive-deps

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

  const matchingTabs = TABS.filter(tab => !query.trim() || Object.values(sections).some(section => section.tab === tab && section.visible));
  useEffect(() => {
    if (matchingTabs.length && !matchingTabs.includes(activeSettingsTab)) selectTab(matchingTabs[0]);
  }, [query, sections, activeSettingsTab]); // Search keeps keyboard navigation on a matching tab.
  const onTabKeyDown = (event: ReactKeyboardEvent, index: number) => {
    const target = event.key === 'ArrowRight' ? (index + 1) % matchingTabs.length
      : event.key === 'ArrowLeft' ? (index + matchingTabs.length - 1) % matchingTabs.length
      : event.key === 'Home' ? 0
      : event.key === 'End' ? matchingTabs.length - 1 : null;
    if (target === null) return;
    event.preventDefault();
    selectTab(matchingTabs[target]);
    tabRefs.current[matchingTabs[target]]?.focus();
  };

  const tabLabels: Record<SettingsTab, string> = {
    general: t('settingsTabGeneral'), catalog: t('settingsTabCatalog'), services: t('settingsTabServices'),
    maintenance: t('settingsTabMaintenance'), info: t('settingsTabInfo'),
  };

  const printersSection = (
    <SettingsSection id="printers" help={t('settingsHelpPrinters')} title={t('settingsSecPrinters')}>
      <p className="text-small text-[var(--ink-2)]">{t('settingsPrintersMovedHint')}</p>
      <button className="mt-2 hover:bg-[var(--panel-2)] hover:text-[var(--ink)] text-[var(--accent)] focus-visible:outline-2" onClick={onOpenPrinters}>{t('pmOpen')}</button>
    </SettingsSection>
  );

  return (
    <SettingsToolsContext.Provider value={tools}>
    <ModalDialogOwner.Provider value={dialogRef}>
    <div ref={dialogRef} role={overlay ? 'dialog' : 'complementary'} aria-modal={overlay ? true : undefined} aria-label={t('settingsTitle')}
      style={{ '--settings-panel-width': layout.width === null ? '100%' : `${layout.width}px`, '--settings-pad': `${layout.pad}px` } as CSSProperties}
      className={`detail-panel-shell settings-panel ${overlay ? 'settings-panel-overlay' : ''}`}>
      {overlay && <div aria-hidden="true" className="settings-panel-scrim" />}
      <aside ref={panelRef} className="settings-panel-aside flex-none flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
        <div className="flex-none flex items-center justify-between settings-panel-pad-x pt-3.5 pb-2.5">
          <h2 className="text-[length:var(--font-size-title)] font-semibold text-[var(--ink)]">{t('settingsTitle')}</h2>
          <button type="button" aria-label={t('settingsClose')} title={`${t('settingsClose')} (Esc)`} onClick={onClose}
            className="min-h-[26px] px-2 grid place-items-center rounded-[3px] text-[var(--ink-3)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
            {t('settingsClose')}
          </button>
        </div>
        <label className="settings-panel-margin-x mb-2 text-small">
          <span className="sr-only">{t('settingsSearch')}</span>
          <input type="search" value={query} onChange={event => setQuery(event.target.value)} placeholder={t('settingsSearch')}
            className="w-full rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] px-2 py-1 focus-visible:outline-2" />
        </label>
        {query.trim() && !matchingTabs.length && <p role="status" className="settings-panel-margin-x mb-2">{t('settingsNoResults')}</p>}
        <div role="tablist" aria-label={t('settingsTabsLabel')} className="flex-none flex flex-wrap p-0.5 gap-0.5 settings-panel-margin-x mb-3 border border-[var(--line)] rounded-[3px] bg-[var(--panel-2)]">
          {matchingTabs.map((key, index) => (
            <button
              key={key}
              ref={node => { tabRefs.current[key] = node; }}
              type="button" role="tab" id={`settings-tab-${key}`} aria-controls="settings-tabpanel"
              aria-label={`${tabLabels[key]}${Object.values(entries).some(entry => entry.tab === key && entry.changed) ? ` (${Object.values(entries).filter(entry => entry.tab === key && entry.changed).length})` : ''}`}
              aria-selected={activeSettingsTab === key} tabIndex={activeSettingsTab === key ? 0 : -1}
              onClick={() => selectTab(key)}
              onKeyDown={event => onTabKeyDown(event, index)}
              className={`${segBase} flex-auto whitespace-nowrap !h-auto min-h-[26px] [@media(pointer:coarse)]:min-h-[44px] !px-2 !py-1 text-caption focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${activeSettingsTab === key ? segActive : segInactive}`}
            >
              {tabLabels[key]}{Object.values(entries).some(entry => entry.tab === key && entry.changed) && <span>{` (${Object.values(entries).filter(entry => entry.tab === key && entry.changed).length})`}</span>}
            </button>
          ))}
        </div>
        <div ref={bodyRef} hidden={!matchingTabs.length} role="tabpanel" id="settings-tabpanel" aria-labelledby={`settings-tab-${activeSettingsTab}`}
          className="flex-1 min-h-0 overflow-y-auto scrollbar-stable settings-panel-pad-x pb-4">
          {visitedTabs.has('general') && (
            <SettingsTabContext.Provider value={{ tab: 'general', label: tabLabels.general }}><div hidden={activeSettingsTab !== 'general'}>
              <SettingsSection id="appearance" help={t('settingsHelpAppearance')} title={t('settingsSecAppearance')}>
                <SettingRow id="theme" value={themeSetting} label={t('themeLegend')} onReset={() => onThemeChange(SETTING_DEFAULTS.theme)}>
                <RadioSegments
                  legend={t('themeLegend')}
                  options={(['system', 'white', 'light', 'slate', 'dark'] as ThemeSetting[]).map(opt => ({
                    value: opt,
                    label: themeLabel(opt),
                  }))}
                  value={themeSetting}
                  onChange={onThemeChange}
                />
                <div className="mt-2 font-medium tabular-nums text-caption leading-relaxed text-[var(--ink-3)]">
                  {themeSetting === 'system'
                    ? t('themeDescriptionSystem')
                    : t('themeDescriptionManual').replace(
                        '{mode}',
                        themeLabel(themeSetting),
                      )}
                </div>

                </SettingRow>
                <SettingRow id="density" value={uiDensity} label={t('densityLegend')} onReset={() => onUiDensityChange(SETTING_DEFAULTS.density)}>
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

                </SettingRow>
                <SettingRow id="display" value={displayPreference} label={t('displayPreferenceTitle')} onReset={() => onDisplayPreferenceChange(SETTING_DEFAULTS.display)}>
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

                </SettingRow>
                {onDetailPanelChange && <SettingRow id="detail" value={detailPanel} label={t('settingsDetailDefault')} onReset={() => onDetailPanelChange(SETTING_DEFAULTS.detail)}>
                <RadioSegments
                  className="mt-4"
                  legend={t('settingsDetailDefault')}
                  options={(['auto', 'pinned'] as const).map(mode => ({
                    value: mode, label: t(mode === 'auto' ? 'detailPanelAuto' : 'detailPanelPinned'),
                  }))}
                  value={detailPanel}
                  onChange={onDetailPanelChange}
                />
                <div className="mt-2 font-medium tabular-nums text-caption leading-relaxed text-[var(--ink-3)]">{t('settingsDetailDefaultHelp')}</div>
                </SettingRow>}
                {onViewChange && <SettingRow id="view" value={view} label={t('settingsView')} onReset={() => onViewChange(SETTING_DEFAULTS.view)}>
                  <label id="settings-view">{t('settingsView')}</label>
                  <Select aria-labelledby="settings-view" value={view} onValueChange={value => onViewChange(value as ViewMode)}>
                    <option value="grid">{t('viewGrid')}</option><option value="groupedGrid">{t('viewFolder')}</option><option value="groupedList">{t('viewList')}</option>
                  </Select>
                </SettingRow>}
                {onSortChange && <SettingRow id="sort" value={sort} label={t('settingsSort')} onReset={() => onSortChange(SETTING_DEFAULTS.sort)}>
                  <label id="settings-sort">{t('settingsSort')}</label>
                  <Select aria-labelledby="settings-sort" value={sort} onValueChange={value => onSortChange(value as SortKey)}>
                    <option value="name">{t('sortName')}</option><option value="imported">{t('sortImported')}</option><option value="modified">{t('sortModified')}</option><option value="viewed">{t('sortLastViewed')}</option><option value="printStatus">{t('sortPrintStatus')}</option><option value="size">{t('sortSize')}</option><option value="vol">{t('sortVolume')}</option>
                  </Select>
                </SettingRow>}
                {onSortDirectionChange && <SettingRow id="direction" value={sortDirection} label={t('settingsDirection')} onReset={() => onSortDirectionChange(SETTING_DEFAULTS.direction)}>
                  <label id="settings-direction">{t('settingsDirection')}</label>
                  <Select aria-labelledby="settings-direction" value={sortDirection} onValueChange={value => onSortDirectionChange(value as SortDirection)}>
                    <option value="asc">{t('settingsAscending')}</option><option value="desc">{t('settingsDescending')}</option>
                  </Select>
                </SettingRow>}
                {onCollapseBundlesChange && <SettingRow id="bundles" value={collapseBundles} label={t('bundleCollapseToggle')} onReset={() => onCollapseBundlesChange(SETTING_DEFAULTS.bundles)}>
                  <ToggleSwitch label={t('bundleCollapseToggle')} checked={collapseBundles} onChange={onCollapseBundlesChange} /> <span>{t('bundleCollapseToggle')}</span>
                </SettingRow>}
              </SettingsSection>

              <SettingsSection id="language" help={t('settingsHelpLanguage')} title={t('settingsSecLanguage')}>
                <SettingRow id="language" value={language} label={t('languageTitle')} onReset={() => setLanguage(SETTING_DEFAULTS.language)}>
                <RadioSegments
                  grid
                  hideLegend
                  legend={t('languageTitle')}
                  options={(['de', 'en', 'es', 'fr'] as Language[]).map(lang => ({ value: lang, label: LANGUAGE_LABELS[lang] }))}
                  value={language}
                  onChange={setLanguage}
                />
                </SettingRow>
              </SettingsSection>

              <SettingsSection id="controls" help={t('settingsHelpControls')} title={t('settingsSecControls')}>
                {!web && <SettingRow id="shortcuts" value={singleKeyShortcuts} label={t('singleKeyShortcutsTitle')} onReset={onSingleKeyShortcutsChange ? () => onSingleKeyShortcutsChange(SETTING_DEFAULTS.shortcuts) : undefined}>
                  <div className="flex items-center justify-between gap-2 text-small">
                    <span className="font-semibold">{t('singleKeyShortcutsTitle')}</span>
                    <ToggleSwitch checked={singleKeyShortcuts} label={t('singleKeyShortcutsTitle')} aria-describedby="single-key-shortcuts-desc"
                      onChange={next => onSingleKeyShortcutsChange?.(next)} />
                  </div>
                  <p id="single-key-shortcuts-desc" className="mt-1 text-small text-[var(--ink-2)] mb-3">{t('singleKeyShortcutsDescription')}</p>
                </SettingRow>}
                <button onClick={onOpenTips} className="w-full min-h-8 py-1 border border-[var(--line-strong)] rounded cursor-pointer">{t('keyboardTipsTitle')}</button>
              </SettingsSection>

              <SettingsSection id="printMethod" help={t('settingsHelpPrintMethod')} title={t('settingsSecPrintMethod')}>
                <PrintTechnologySettings />
              </SettingsSection>

              {web && printersSection}

              {!web && (
                <SettingsSection id="program" help={t('settingsHelpProgram')} advanced title={t('settingsSecProgram')} hideWhenEmpty>
                  {!container && <CloseBehaviorSettings />}
                  <LinuxMenuSettings />
                </SettingsSection>
              )}
            </div></SettingsTabContext.Provider>
          )}

          {visitedTabs.has('catalog') && (
            <SettingsTabContext.Provider value={{ tab: 'catalog', label: tabLabels.catalog }}><div hidden={activeSettingsTab !== 'catalog'}>
              {!web && (
                <>
                  <SettingsSection id="storage" help={t('settingsHelpStorage')} title={t('settingsSecStorage')}>
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
                  <SettingsSection id="sources" help={t('settingsHelpSources')} title={t('settingsSecSources')}><SourceFoldersSettings /></SettingsSection>
                  <SettingsSection id="import" help={t('settingsHelpImport')} title={t('settingsSecImport')}><ImportRenameSetting /></SettingsSection>
                  <SettingsSection id="sync" help={t('settingsHelpSync')} title={t('settingsSecSync')}><DiskSyncSettings /></SettingsSection>
                </>
              )}
              <SettingsSection id="autoTags" help={t('settingsHelpAutoTags')} title={t('settingsSecAutoTags')}><AutoTaggingSetting /></SettingsSection>
              <SettingsSection id="printData" help={t('settingsHelpPrintData')} title={t('settingsSecPrintData')}><PrintDataRereadSettings /></SettingsSection>
            </div></SettingsTabContext.Provider>
          )}

          {!web && visitedTabs.has('services') && (
            <SettingsTabContext.Provider value={{ tab: 'services', label: tabLabels.services }}><div hidden={activeSettingsTab !== 'services'}>
              {container && <p className="mb-2 p-2 rounded border border-[var(--line)] bg-[var(--panel-2)] text-small text-[var(--ink-2)]">{t('containerSlicerUnavailable')}</p>}
              {!container && (
                <>
                  <SettingsSection id="handover" help={t('settingsHelpHandover')} advanced title={t('settingsSecHandover')} experimental><DragOutSetting /></SettingsSection>
                  <SettingsSection id="slicers" help={t('settingsHelpSlicers')} title={t('settingsSecSlicers')}>
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
                                className="flex-none px-2 min-h-8 [@media(pointer:coarse)]:w-11 [@media(pointer:coarse)]:h-11 grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
                              >
                                {t('sfRemove')}
                              </button>
                            </div>
                          );
                        })}
                        </div>
                      </fieldset>
                    )}
                    <SettingActions id="primarySlicer" value={primarySlicerId} label={t('slicerDefaultLegend')} />
                    <button ref={addSlicerRef} onClick={onAddSlicer} className={`mt-2 ${dashedButton}`}>
                      {t('addSlicer')}
                    </button>
                    {addSlicerError && (
                      <div className="mt-1.5 font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--accent)] break-words">
                        {t('addSlicerError')} <ErrorText error={addSlicerError} />
                      </div>
                    )}
                  </SettingsSection>
                  <SettingsSection id="slicerReports" help={t('settingsHelpSlicerReports')} advanced title={t('settingsSecSlicerReports')}><SlicerReportSettings active={activeSettingsTab === 'services'} /></SettingsSection>
                </>
              )}
              {printersSection}
              <SettingsSection id="thingiverse" help={t('settingsHelpThingiverse')} advanced title={t('settingsSecThingiverse')} experimental>
                <ThingiverseSettings onEnabledChange={enabled => onThingiverseEnabledChange?.(enabled)} />
              </SettingsSection>
            </div></SettingsTabContext.Provider>
          )}

          {visitedTabs.has('maintenance') && <SettingsTabContext.Provider value={{ tab: 'maintenance', label: tabLabels.maintenance }}><div hidden={activeSettingsTab !== 'maintenance'}>{web ? (
            <SettingsSection id="webBackup" help={t('settingsHelpWebBackup')} title={t('settingsSecWebBackup')}><WebCatalogBackup /></SettingsSection>
          ) : (
            <>
              <SettingsSection id="check" help={t('settingsHelpCheck')} title={t('settingsSecCheck')}>
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

              <SettingsSection id="backup" help={t('settingsHelpBackup')} title={t('settingsSecBackup')}>
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

              <SettingsSection id="reset" help={t('settingsHelpReset')} title={t('settingsSecReset')} danger>
                <CatalogResetSection modelCount={catalogModelCount} folderCount={catalogFolderCount}
                  onExport={onExportCatalog} onReset={onCatalogReset} backupError={catalogBackupError} />
              </SettingsSection>
            </>
          )}</div></SettingsTabContext.Provider>}

          {visitedTabs.has('info') && (
            <SettingsTabContext.Provider value={{ tab: 'info', label: tabLabels.info }}><div hidden={activeSettingsTab !== 'info'}>
              <SettingsSection id="version" help={t('settingsHelpVersion')} title={t(web ? 'settingsSecVersionWeb' : 'settingsSecVersion')}>
                {web ? <WebVersionRow /> : <UpdatePanel view={update} />}
                <button className="mt-2 w-full min-h-8 border border-[var(--line-strong)] rounded cursor-pointer" onClick={onOpenWhatsNew}>{t('whatsNew')}</button>
              </SettingsSection>
              {!web && !container && update.preview === false && (
                <SettingsSection id="preRelease" help={t('settingsHelpPreRelease')} advanced title={t('settingsSecPreRelease')}>
                  <UpdateChannelControl disabled={isUpdateCheckDisabled(update.phase)} onExport={onExportCatalog} exportError={catalogBackupError} onChanged={update.checkNow} />
                </SettingsSection>
              )}
              {!web && <SettingsSection id="help" help={t('settingsHelpHelp')} advanced title={t('settingsSecHelp')}><DiagnosticsSettings /></SettingsSection>}
              {!web && <SettingsSection id="appData" help={t('settingsHelpAppData')} advanced title={t('settingsSecAppData')}><InfoFolders /></SettingsSection>}
              <SettingsSection id="about" help={t('settingsHelpAbout')} title={t('settingsSecAbout')}>
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
            </div></SettingsTabContext.Provider>
          )}
        </div>
        <footer className="flex-none settings-panel-pad-x py-2 border-t border-[var(--line)] text-small">
          <span role="status">{t('settingsChangedCount').replace('{n}', String(count))}</span>{' · '}
          <button className="underline disabled:opacity-40" disabled={!resettable.length || resetting} onClick={() => setConfirmReset(true)}>{t('settingsResetAll')}</button>
          {Object.values(entries).some(entry => !entry.baselineKnown) && <p className="mt-1 text-caption text-[var(--ink-2)]">{t('settingsBaselineUnknown')}</p>}
        </footer>
        {confirmReset && <CatalogActionDialog title={t('settingsResetAll')} onClose={() => { if (!resetting) setConfirmReset(false); }}>
          <p>{t('settingsResetQuestion')}</p>
          <button className={catalogActionButton} disabled={resetting} onClick={() => void resetAll()}>{t('settingsResetAll')}</button>
          <button className={catalogActionButton} data-initial-focus disabled={resetting} onClick={() => setConfirmReset(false)}>{t('cancel')}</button>
        </CatalogActionDialog>}
      </aside>
    </div>
    </ModalDialogOwner.Provider>
    </SettingsToolsContext.Provider>
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
