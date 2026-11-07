import { useEffect, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { getVerboseLogging, setVerboseLogging } from '../lib/api/diagnostics';
import { toAppError, type AppError } from '../lib/errors';
import { SettingLoadStatus, SettingSaveStatus, useLoadedSetting } from '../components/SettingState';
import { ToggleSwitch } from '../components/ToggleSwitch';
import { useDiagnostics } from './DiagnosticsContext';
import { BUG_ICON } from './icons';

export function DiagnosticsSettings() {
  const t = useT();
  const { language } = useLanguage();
  const { openBugReport } = useDiagnostics();
  // Until the stored state is known the switch stays off-limits: a guessed "off" would be a claim.
  const { value: verbose, confirm, state, error: loadError, reload } = useLoadedSetting(getVerboseLogging);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const change = async (on: boolean) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await setVerboseLogging(on);
      if (mountedRef.current) confirm(next);
    } catch (e) {
      if (mountedRef.current) setError(toAppError(e));
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  };

  const until = verbose?.untilMs
    ? new Date(verbose.untilMs).toLocaleDateString(language, { day: '2-digit', month: '2-digit', year: 'numeric' })
    : '';

  return (
    <>
      <button
        type="button"
        onClick={openBugReport}
        className="mt-2 h-8 w-full flex items-center justify-center gap-2 rounded-[3px] border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] text-small font-semibold cursor-pointer"
      >
        {BUG_ICON}
        {t('infoReportBugButton')}
      </button>
      <div className="mt-3 pt-3 border-t border-[var(--line)]">
        <div className="flex items-center justify-between text-caption text-[var(--ink)]">
          <span>{t('infoVerboseLoggingLabel')}</span>
          <ToggleSwitch
            checked={verbose?.enabled === true}
            label={t('infoVerboseLoggingLabel')}
            disabled={!verbose || busy}
            onChange={(on) => void change(on)}
          />
        </div>
        <p className={`mt-1 text-caption leading-[1.4] ${verbose?.enabled ? 'text-[var(--warn)]' : 'text-[var(--ink-3)]'}`}>
          {verbose?.enabled ? t('infoVerboseLoggingOn').replace('{date}', until) : t('infoVerboseLoggingHint')}
        </p>
        <SettingLoadStatus state={state} error={loadError} onReload={() => void reload()} className="mt-1" />
        <SettingSaveStatus saving={busy} error={error} keptValue className="mt-1" />
      </div>
    </>
  );
}
