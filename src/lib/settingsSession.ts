import { uiSettings } from './uiSettings';
import { invoke } from './transport';

// Defaults mirror the existing hook initializers and backend getters. Never reset
// resource lists, credentials or paths: these have no universal default.
export const SETTING_DEFAULTS = {
  theme: 'system', density: 'compact', display: 'thumbnail', language: 'de',
  detail: 'auto', shortcuts: true, view: 'grid', sort: 'name', direction: 'asc', bundles: true,
  printMethod: 'both', importRename: false, sourceCheck: false, syncStart: false, syncAdd: false,
  autoTags: true, dragOut: false, close: 'exit', reports: false, thingiverse: false,
  channel: 'stable', verbose: false,
} as const;
export type SettingId = keyof typeof SETTING_DEFAULTS | 'linuxMenu' | 'primarySlicer';
export function settingChanged(current: unknown, baseline: unknown): boolean {
  return baseline !== undefined && current !== undefined && !Object.is(current, baseline);
}
export function normalizeSettingsSearch(text: string): string {
  return text.toLowerCase().replace(/ß/g, 'ss').normalize('NFD').replace(/[\u0300-\u036f]/g, '').trim();
}
export function matchesSettingsSearch(query: string, ...texts: string[]): boolean {
  const text = normalizeSettingsSearch(texts.join(' '));
  return normalizeSettingsSearch(query).split(/\s+/).every(word => text.includes(word));
}
const baseline = new Map<SettingId, unknown>();
let version = 0;
const listeners = new Set<() => void>();
const bump = () => { version++; listeners.forEach(l => l()); };
export const subscribeSettingsBaseline = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
export const settingsBaselineVersion = () => version;
export const settingBaseline = (id: SettingId) => baseline.get(id);
let startup: Promise<void> | undefined;
const uiKeys = {
  theme: 'theme', density: 'density', display: 'display-preference', language: 'language',
  detail: 'detail-panel', shortcuts: 'single-key-shortcuts', view: 'view', bundles: 'bundle-collapse',
} as const;
/** Freeze before the application mounts, not when the settings dialog opens. */
export function captureUiSettingsBaseline() {
  for (const [id, suffix] of Object.entries(uiKeys)) {
    const key = id as keyof typeof uiKeys;
    if (baseline.has(key)) continue;
    const stored = uiSettings.getItem(`3mf-katalog-${suffix}`);
    const value = key === 'bundles' ? stored !== 'off' : key === 'shortcuts' ? stored !== 'false'
      : key === 'theme' ? (['white', 'light', 'slate', 'dark'].includes(stored ?? '') ? stored : 'system')
      : key === 'density' ? (stored === 'comfort' ? stored : 'compact')
      : key === 'display' ? (stored === 'render' ? stored : 'thumbnail')
      : key === 'detail' ? (stored === 'pinned' ? stored : 'auto')
      : key === 'view' ? (['groupedGrid', 'groupedList'].includes(stored ?? '') ? stored : 'grid')
      : (['de', 'en', 'es', 'fr'].includes(stored ?? '') ? stored : 'de');
    baseline.set(key, value);
  }
  // No stored slicer: useSlicers auto-selects one later. Leave the base unknown (= not changed).
  const slicer = uiSettings.getItem('3mf-katalog-primary-slicer-id');
  if (!baseline.has('primarySlicer') && slicer !== null) baseline.set('primarySlicer', slicer);
  baseline.set('sort', SETTING_DEFAULTS.sort);
  baseline.set('direction', SETTING_DEFAULTS.direction);
}
const backendReads: [SettingId, string, string?][] = [
  ['printMethod', 'get_print_technology', 'value'], ['autoTags', 'get_auto_tagging'],
  ['importRename', 'get_import_rename_enabled'], ['sourceCheck', 'get_source_check_on_start'],
  ['syncStart', 'get_sync_on_start'], ['syncAdd', 'get_sync_auto_add'],
  ['dragOut', 'get_drag_out_enabled'], ['close', 'get_close_behavior', 'action'],
  ['linuxMenu', 'get_linux_menu_status', 'installed'], ['reports', 'get_slicer_report_setup', 'enabled'],
  ['thingiverse', 'get_thingiverse_settings', 'enabled'], ['channel', 'get_update_channel'],
  ['verbose', 'get_verbose_logging', 'enabled'],
];
export function captureSettingsBaseline(): Promise<void> {
  if (startup) return startup;
  captureUiSettingsBaseline();
  startup = Promise.all(backendReads.filter(([id]) => import.meta.env.VITE_TARGET !== 'web' || ['printMethod', 'autoTags'].includes(id)).map(async ([id, command, field]) => {
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      const value = await Promise.race([invoke<unknown>(command), new Promise<never>((_, reject) => { timeout = setTimeout(() => reject(), 3000); })]);
      const scalar = field ? (value as Record<string, unknown> | null)?.[field] : value;
      if ((typeof scalar === 'boolean' || typeof scalar === 'string')) baseline.set(id, scalar);
    } catch { /* Unknown startup values must not be presented as unchanged defaults. */ }
    finally { clearTimeout(timeout); }
  })).then(() => { bump(); });
  return startup;
}
/** Isolate application sessions in tests. */
export function resetSettingsBaseline() { startup = undefined; baseline.clear(); bump(); }
