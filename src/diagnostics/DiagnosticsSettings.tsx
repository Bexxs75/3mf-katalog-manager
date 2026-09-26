import { useEffect, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { getVerboseLogging, openLogFolder, setVerboseLogging, type VerboseLogging } from '../lib/api/diagnostics';
import { ToggleSwitch } from '../components/ToggleSwitch';
import { useDiagnostics } from './DiagnosticsContext';

const BUG_ICON = (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M8 2l1.9 1.9M16 2l-1.9 1.9M9 7.1V6a3 3 0 1 1 6 0v1.1" />
    <path d="M12 20c-3.3 0-6-2.7-6-6v-3a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v3c0 3.3-2.7 6-6 6z" />
    <path d="M12 20v-9M6.5 9C4.6 8.8 3 7.1 3 5M6 13H2M3 21c0-2.1 1.7-3.9 3.8-4M20.97 5c0 2.1-1.6 3.8-3.5 4M22 13h-4M17.2 17c2.1.1 3.8 1.9 3.8 4" />
  </svg>
);

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

  const until = verbose.untilMs ? new Date(verbose.untilMs).toLocaleDateString(language) : '';

  return (
    <>
      <button
        type="button"
        onClick={openBugReport}
        className="mt-2 h-8 w-full flex items-center justify-center gap-2 rounded-[3px] border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] text-[12px] font-semibold cursor-pointer"
      >
        {BUG_ICON}
        {t('infoReportBugButton')}
      </button>
      <div className="mt-3 pt-3 border-t border-[var(--line)]">
        <div className="font-mono-ui text-[10px] tracking-[0.14em] text-[var(--ink-3)]">{t('infoDiagnosticsHeading')}</div>
        <div className="mt-2 flex items-center justify-between text-[11.5px] text-[var(--ink)]">
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
        <p className={`mt-1 text-[10.5px] leading-[1.4] ${verbose.enabled ? 'text-[var(--warn)]' : 'text-[var(--ink-3)]'}`}>
          {verbose.enabled ? t('infoVerboseLoggingOn').replace('{date}', until) : t('infoVerboseLoggingHint')}
        </p>
        <div className="mt-1.5 flex justify-between text-[11.5px] text-[var(--ink-2)]">
          <span>{t('infoLogFilesLabel')}</span>
          <button type="button" onClick={() => openLogFolder().catch(() => {})} className="underline cursor-pointer bg-transparent border-0 p-0 text-[var(--ink-2)]">
            {t('infoOpenFolder')}
          </button>
        </div>
      </div>
    </>
  );
}
