import { createContext, useContext, useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import { useT } from '../i18n/LanguageContext';
import { SETTING_DEFAULTS, settingBaseline, settingsBaselineVersion, subscribeSettingsBaseline, settingChanged, matchesSettingsSearch, type SettingId } from '../lib/settingsSession';
import type { SettingsTab } from '../lib/settingsNav';

export interface SettingEntry { changed: boolean; baselineKnown: boolean; reset?: () => void | Promise<unknown>; disabled: boolean }
export const SettingsToolsContext = createContext<{
  query: string;
  report: (id: SettingId, tab: SettingsTab, entry: SettingEntry | null) => void;
  reportSection: (id: string, tab: SettingsTab, visible: boolean | null) => void;
} | null>(null);
export const SettingsTabContext = createContext<{ tab: SettingsTab; label: string }>({ tab: 'general', label: '' });
export const SettingsSectionSearch = createContext('');

export function SettingActions({ id, value, label, onReset, disabled = false }: {
  id: SettingId; value: unknown; label: string; onReset?: () => void | Promise<unknown>; disabled?: boolean;
}) {
  const t = useT();
  useSyncExternalStore(subscribeSettingsBaseline, settingsBaselineVersion); // re-render when the backend baseline arrives
  const tools = useContext(SettingsToolsContext);
  const { tab } = useContext(SettingsTabContext);
  const baselineKnown = id === 'primarySlicer' || value === undefined || value === null || settingBaseline(id) !== undefined;
  const changed = settingChanged(value, settingBaseline(id));
  const action = useRef(onReset);
  action.current = onReset;
  const canReset = !!onReset && id in SETTING_DEFAULTS;
  const isDefault = canReset && Object.is(value, SETTING_DEFAULTS[id as keyof typeof SETTING_DEFAULTS]);
  const blocked = disabled || value === undefined || value === null;
  useEffect(() => {
    tools?.report(id, tab, { changed, baselineKnown, disabled: blocked || !!isDefault,
      reset: canReset ? () => action.current?.() : undefined });
    return () => tools?.report(id, tab, null);
  }, [tools?.report, id, tab, changed, baselineKnown, blocked, isDefault, canReset]);
  if (!tools) return null;
  return <div data-setting-actions className="flex items-center gap-2 text-caption mt-1 mb-2">
    {changed && <span role="img" aria-label={`${label}: ${t('settingsChanged')}`} className="inline-flex items-center gap-1 text-[var(--ink-2)]">
      <span aria-hidden="true" className="inline-block w-1.5 h-1.5 rounded-full bg-[var(--accent)]" />{t('settingsChanged')}
    </span>}
    {canReset && <button type="button" disabled={blocked || !!isDefault} aria-label={`${t('settingsReset')}: ${label}`}
      className="ml-auto underline text-[var(--ink-2)] disabled:opacity-40 focus-visible:outline-2" onClick={() => void onReset?.()}>{t('settingsReset')}</button>}
  </div>;
}

/** A searchable preference row, keeping its own description and control together. */
/** `keywords` are extra search terms that the visible texts do not contain (alternative names of a feature). */
export function SettingRow({ id, value, label, onReset, disabled, keywords = '', children }: Parameters<typeof SettingActions>[0] & { keywords?: string; children: ReactNode }) {
  const tools = useContext(SettingsToolsContext);
  const { label: tabLabel } = useContext(SettingsTabContext);
  const section = useContext(SettingsSectionSearch);
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(true);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const update = () => setVisible(matchesSettingsSearch(tools?.query ?? '', tabLabel, section, label, node.querySelector('[data-setting-content]')?.textContent ?? '', keywords));
    update();
    const observer = new MutationObserver(update);
    observer.observe(node, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [tools?.query, tabLabel, section, label, keywords]);
  return <div ref={ref} hidden={!visible} data-setting-row>
    <div data-setting-content>{children}</div>
    <SettingActions id={id} value={value} label={label} onReset={onReset} disabled={disabled} />
  </div>;
}
