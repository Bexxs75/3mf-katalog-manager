import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import * as api from '../lib/api/closeBehavior';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';

/** Radio group "when the window is closed"; renders nothing where there is no notification area. */
export function CloseBehaviorSettings() {
  const t = useT();
  const [state, setState] = useState<api.CloseBehavior | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const mountedRef = useRef(true);
  // Set to true again on setup: StrictMode runs setup, cleanup, setup.
  useEffect(() => { mountedRef.current = true; return () => { mountedRef.current = false; }; }, []);
  useEffect(() => {
    api.getCloseBehavior().then(s => mountedRef.current && setState(s)).catch(() => {});
  }, []);
  if (!state) return null;
  const mac = state.platform === 'macos';
  const linux = state.platform === 'linux';
  // The wish alone hides nothing: without a built icon the window closes as before.
  const unavailable = state.action === 'tray' && !state.trayAvailable;
  const shown = unavailable ? 'exit' : state.action;
  async function choose(action: api.CloseAction) {
    setBusy(true);
    setError(null);
    try {
      const next = await api.setCloseBehavior(action);
      if (mountedRef.current && next) setState(next);
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
  return <fieldset className="mt-4" disabled={busy} aria-describedby="close-action-help">
    <legend className="text-[length:var(--font-size-body)] font-semibold">{t('closeActionTitle')}</legend>
    {options.map(option => <label key={option.value} className="flex items-start gap-2 mt-1">
      <input type="radio" name="closeAction" className="mt-1" checked={shown === option.value} onChange={() => void choose(option.value)} />
      {option.label}
    </label>)}
    <p id="close-action-help" className="mt-1 text-caption leading-[1.4] text-[var(--ink-3)]">{t(mac ? 'closeActionHelpMac' : 'closeActionHelp')}</p>
    {linux && <p className="mt-1 text-caption leading-[1.4] text-[var(--ink-2)]"><b>{t('closeActionNoteLabel')}</b> {t('closeActionLinuxWarning')}</p>}
    {unavailable && <p role="alert" className="mt-1 text-caption text-[var(--crit)]">{t('closeActionUnavailable')}</p>}
    {error && <p role="alert" className="mt-1 text-caption text-[var(--crit)]">{t('closeActionFailed')} <ErrorText error={error} /></p>}
  </fieldset>;
}
