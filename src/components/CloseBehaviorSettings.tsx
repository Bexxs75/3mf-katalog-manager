import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import * as api from '../lib/api/closeBehavior';
import { toAppError, type AppError } from '../lib/errors';
import { useSectionContent } from './SettingsSection';
import { SettingLoadStatus, SettingSaveStatus, useLoadedSetting } from './SettingState';

/** Radio group "when the window is closed"; renders nothing where there is no notification area. */
export function CloseBehaviorSettings() {
  const t = useT();
  const { value: state, confirm, state: loadState, error: loadError, reload } = useLoadedSetting(api.getCloseBehavior);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const mountedRef = useRef(true);
  // Set to true again on setup: StrictMode runs setup, cleanup, setup.
  useEffect(() => { mountedRef.current = true; return () => { mountedRef.current = false; }; }, []);
  // Tells a section around this block whether there is anything to show: a read in progress or failed is, a loaded "no" is not.
  useSectionContent(loadState !== 'ready' || Boolean(state));
  // `null` is the loaded answer "no notification area": the group stays hidden then.
  if (!state && loadState !== 'ready') return <SettingLoadStatus state={loadState} label={t('closeActionTitle')} error={loadError} onReload={() => void reload()} className="mt-1" />;
  if (!state) return null;
  const mac = state.platform === 'macos';
  const linux = state.platform === 'linux';
  // The wish alone hides nothing: without a built icon the window closes as before.
  const unavailable = state.action === 'tray' && !state.trayAvailable;
  const shown = unavailable ? 'exit' : state.action;
  async function choose(action: api.CloseAction) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await api.setCloseBehavior(action);
      if (mountedRef.current && next) confirm(next);
    } catch (e) {
      if (mountedRef.current) setError(toAppError(e));
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  }
  const options: { value: api.CloseAction; label: string }[] = [
    { value: 'exit', label: t('closeActionExit') },
    { value: 'tray', label: t(mac ? 'closeActionTrayMac' : linux ? 'closeActionTrayLinux' : 'closeActionTray') },
  ];
  return <>
    <SettingLoadStatus keptValue state={loadState} error={loadError} onReload={() => void reload()} />
    <fieldset disabled={loadState !== 'ready'} aria-busy={busy} className={busy ? 'opacity-60' : undefined} aria-describedby="close-action-help">
    <legend className="text-[length:var(--font-size-body)] font-semibold">{t('closeActionTitle')}</legend>
    {options.map(option => <label key={option.value} className="flex items-start gap-2 mt-1">
      <input type="radio" name="closeAction" className="mt-1 print-technology-radio focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]" checked={shown === option.value} onChange={() => void choose(option.value)} />
      {option.label}
    </label>)}
    <p id="close-action-help" className="mt-1 text-caption leading-[1.4] text-[var(--ink-3)]">{t(mac ? 'closeActionHelpMac' : 'closeActionHelp')}</p>
    {linux && <p className="mt-1 text-caption leading-[1.4] text-[var(--ink-2)]"><b>{t('closeActionNoteLabel')}</b> {t('closeActionLinuxWarning')}</p>}
    {unavailable && <p role="alert" className="mt-1 text-caption text-[var(--crit)]">{t('closeActionUnavailable')}</p>}
    <SettingSaveStatus saving={busy} error={error} keptValue className="mt-1" />
  </fieldset></>;
}
