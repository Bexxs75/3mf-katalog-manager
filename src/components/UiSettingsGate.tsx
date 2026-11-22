import { captureSettingsBaseline, captureUiSettingsBaseline } from '../lib/settingsSession';
import { useEffect, useState, useSyncExternalStore, type ReactNode } from 'react';
import { clearUiSettingsError, getUiSettingsError, initializeUiSettings, isDesktopUiSettings, subscribeUiSettings } from '../lib/uiSettings';
import { useT } from '../i18n/LanguageContext';

export function UiSettingsGate({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(() => { if (isDesktopUiSettings() || import.meta.env.VITE_TARGET === 'web') return false; captureUiSettingsBaseline(); return true; });
  useEffect(() => {
    let active = true;
    void initializeUiSettings().then(() => {
      captureUiSettingsBaseline();
      if (active) setReady(true);
      // Backend values load in the background; the dialog treats them as unknown until then.
      if (isDesktopUiSettings() || import.meta.env.VITE_TARGET === 'web') void captureSettingsBaseline();
    });
    return () => { active = false; };
  }, []);
  return ready ? children : null;
}

export function UiSettingsError() {
  const error = useSyncExternalStore(subscribeUiSettings, getUiSettingsError);
  const t = useT();
  if (!error) return null;
  return <div role="alert" className="fixed top-2 left-2 right-2 z-[100] border rounded bg-[var(--panel)] text-[var(--crit)] p-3">
    {t(error === 'uiSettingsInvalid' ? 'errUiSettingsInvalid' : 'errUiSettingsUnavailable')}
    <button className="ml-3 underline" onClick={clearUiSettingsError}>{t('diskSyncClose')}</button>
  </div>;
}
