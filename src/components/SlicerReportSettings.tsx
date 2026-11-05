import { useCallback, useEffect, useRef, useState } from 'react';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import * as api from '../lib/api/slicerReports';
import type { SlicerReportSetup } from '../lib/slicerReports';
import { useSlicerReports } from '../hooks/SlicerReportsContext';
import { ToggleSwitch } from './ToggleSwitch';

const button = 'h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] disabled:opacity-50 disabled:cursor-default focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/**
 * Settings → Slicer: switch for the report intake, one command line per slicer (copy it into the post-processing
 * script of the slicer) and a test report. Desktop only. Nothing here assigns or moves a file.
 */
export function SlicerReportSettings() {
  const t = useT();
  const formatCount = useFormatCount();
  const { open, openInbox, refresh } = useSlicerReports();
  const [setup, setSetup] = useState<SlicerReportSetup | null>(null);
  const [status, setStatus] = useState('');
  const [error, setError] = useState<{ text: string; error: AppError } | null>(null);
  const [busy, setBusy] = useState(false);
  const alive = useRef(true);

  const load = useCallback(async () => {
    try {
      const next = await api.getSlicerReportSetup();
      if (alive.current) setSetup(next);
    } catch (e) {
      console.warn('[slicer-reports] could not read the setup:', e);
    }
  }, []);
  useEffect(() => {
    alive.current = true;
    void load();
    return () => { alive.current = false; };
  }, [load]);
  // A report that arrives while the page is open changes the count and the last error.
  useEffect(() => { void load(); }, [open.length, load]);

  const setEnabled = async (enabled: boolean, regenerate = false) => {
    setBusy(true);
    setError(null);
    setStatus('');
    try {
      const next = await api.setSlicerReportEnabled(enabled);
      setSetup(next);
      if (regenerate) setStatus(t('srRegenerated'));
      void refresh();
    } catch (e) {
      setError({ text: t('srSaveFailed'), error: toAppError(e) });
    } finally {
      setBusy(false);
    }
  };

  const sendTest = async () => {
    setError(null);
    setStatus('');
    try {
      await api.sendSlicerTestReport();
      await refresh();
      await load();
      setStatus(t('srTestSent'));
    } catch (e) {
      setError({ text: t('srTestFailed'), error: toAppError(e) });
    }
  };

  const copy = async (slicer: string, line: string, input: HTMLInputElement | null) => {
    setError(null);
    try {
      if (!navigator.clipboard?.writeText) throw new Error('clipboard unavailable');
      await navigator.clipboard.writeText(line);
      setStatus(t('srCopied').replace('{slicer}', slicer));
    } catch {
      input?.select();
      setStatus(t('srCopyFailed'));
    }
  };

  const enabled = setup?.enabled === true;
  return (
    <section className="mb-4" aria-labelledby="sr-settings-title">
      <div className="flex items-center justify-between gap-3 mb-1">
        <span id="sr-settings-title" className="text-[length:var(--font-size-body)] font-semibold">{t('srTitle')}</span>
        <ToggleSwitch checked={enabled} label={t('srTitle')} disabled={busy || setup === null} onChange={(next) => void setEnabled(next)} />
      </div>
      <p className="text-caption leading-relaxed text-[var(--ink-3)] mb-2">{t('srHelp')}</p>

      {enabled && setup?.pathChanged && (
        <div role="alert" className="mb-2 p-2 border border-[var(--crit)] rounded text-caption">
          <b>{t('srPathChangedTitle')}</b> {t('srPathChangedText')}
          <div className="mt-1.5"><button type="button" className={button} disabled={busy} onClick={() => void setEnabled(true, true)}>{t('srRegenerate')}</button></div>
        </div>
      )}

      {enabled && setup && (
        <>
          <ol className="list-decimal pl-5 mb-2 text-caption text-[var(--ink-2)] space-y-0.5">
            <li>{t('srStep1')}</li>
            <li>{t('srStep2')}</li>
          </ol>
          <ul className="space-y-1.5 mb-2">
            {setup.commands.map((c, i) => {
              const id = `sr-cmd-${i}`;
              return (
                <li key={c.slicer} className={setup.pathChanged ? 'opacity-60' : undefined}>
                  <label htmlFor={id} className="block text-caption text-[var(--ink-2)] mb-0.5">{t('srCommandFor').replace('{slicer}', c.slicer)}</label>
                  {c.commandLine === null
                    ? <p id={id} className="text-caption text-[var(--ink-3)]">{t('srCommandUnavailable')}</p>
                    : (
                      <div className="flex gap-1.5">
                        <input id={id} readOnly value={c.commandLine} onFocus={(e) => e.currentTarget.select()}
                          className="flex-1 min-w-0 h-7 px-2 rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink)] text-caption font-mono" />
                        <button type="button" className={button}
                          onClick={(e) => void copy(c.slicer, c.commandLine!, e.currentTarget.parentElement?.querySelector('input') ?? null)}>
                          {t('srCopy')}
                        </button>
                      </div>
                    )}
                </li>
              );
            })}
          </ul>
          <div className="flex flex-wrap gap-2 items-center">
            <button type="button" className={button} onClick={() => void sendTest()}>{t('srTest')}</button>
            <button type="button" className={button} onClick={openInbox}>{formatCount(t('srOpenInbox'), open.length)}</button>
          </div>
        </>
      )}
      {!enabled && setup && <p className="text-caption text-[var(--ink-3)]">{t('srOffNote')}</p>}
      {setup?.lastError && <p className="mt-1 text-caption text-[var(--ink-3)] [overflow-wrap:anywhere]">{t('srLastError').replace('{message}', setup.lastError)}</p>}
      <p role="status" aria-live="polite" className="mt-1 text-caption text-[var(--ink-2)]">{status}</p>
      {error && <p role="alert" className="text-caption text-[var(--crit)]">{error.text} <ErrorText error={error.error} /></p>}
    </section>
  );
}
