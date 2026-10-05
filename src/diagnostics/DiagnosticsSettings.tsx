import { useEffect, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { getVerboseLogging, setVerboseLogging, type VerboseLogging } from '../lib/api/diagnostics';
import { ToggleSwitch } from '../components/ToggleSwitch';
import { useDiagnostics } from './DiagnosticsContext';
import { BUG_ICON } from './icons';

export function DiagnosticsSettings() {
  const t = useT();
  const { language } = useLanguage();
  const { openBugReport } = useDiagnostics();
  const [verbose, setVerbose] = useState<VerboseLogging>({ enabled: false, untilMs: null });
  const mountedRef = useRef(true);

  useEffect(
    () => () => {
      mountedRef.current = false;
    },
    [],
  );

  useEffect(() => {
    getVerboseLogging()
      .then((v) => mountedRef.current && setVerbose(v))
      .catch(() => {});
  }, []);

  const until = verbose.untilMs
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
        <div className="ui-label text-[var(--ink-3)]">{t('infoDiagnosticsHeading')}</div>
        <div className="mt-2 flex items-center justify-between text-caption text-[var(--ink)]">
          <span>{t('infoVerboseLoggingLabel')}</span>
          <ToggleSwitch
            checked={verbose.enabled}
            label={t('infoVerboseLoggingLabel')}
            onChange={(on) =>
              setVerboseLogging(on)
                .then((v) => mountedRef.current && setVerbose(v))
                .catch(() => {})
            }
          />
        </div>
        <p className={`mt-1 text-caption leading-[1.4] ${verbose.enabled ? 'text-[var(--warn)]' : 'text-[var(--ink-3)]'}`}>
          {verbose.enabled ? t('infoVerboseLoggingOn').replace('{date}', until) : t('infoVerboseLoggingHint')}
        </p>
      </div>
    </>
  );
}
