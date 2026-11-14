/** Tabs of the settings panel, in display order. */
export type SettingsTab = 'general' | 'catalog' | 'services' | 'maintenance' | 'info';

/** Every section of the panel; the id names the heading element other parts of the app can jump to. */
export type SettingsSectionId =
  | 'appearance' | 'language' | 'controls' | 'printMethod' | 'printers' | 'program'
  | 'storage' | 'sources' | 'import' | 'sync' | 'autoTags' | 'printData'
  | 'handover' | 'slicers' | 'slicerReports' | 'thingiverse'
  | 'check' | 'backup' | 'webBackup' | 'reset'
  | 'version' | 'preRelease' | 'help' | 'appData' | 'about';

/** What the rest of the app can ask the settings to show. Each target is one section. */
export type SettingsTarget = 'printMethod' | 'sourceFolders' | 'printData' | 'slicers' | 'dragOut' | 'thingiverse' | 'storage';

export interface SettingsRequest {
  target: SettingsTarget;
  /** Changes with every request so asking for the same target twice still moves the panel. */
  nonce: number;
}

export const SETTINGS_TARGETS: Record<SettingsTarget, { tab: SettingsTab; section: SettingsSectionId }> = {
  printMethod: { tab: 'general', section: 'printMethod' },
  sourceFolders: { tab: 'catalog', section: 'sources' },
  printData: { tab: 'catalog', section: 'printData' },
  storage: { tab: 'catalog', section: 'storage' },
  slicers: { tab: 'services', section: 'slicers' },
  dragOut: { tab: 'services', section: 'handover' },
  thingiverse: { tab: 'services', section: 'thingiverse' },
};

export const settingsHeadingId = (section: SettingsSectionId) => `settings-sec-${section}`;
