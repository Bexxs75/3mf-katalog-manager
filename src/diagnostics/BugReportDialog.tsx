import { ExternalLink } from '../components/ExternalLink';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { useModalDialog } from '../hooks/useModalDialog';
import { useEffect, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import {
  getBugReportInfo,
  getBugReportUrl,
  openBugReportForm,
  previewLogExport,
  saveLogExport,
  type BugReportInfo,
  type LogPreview,
} from '../lib/api/diagnostics';
import { messageOf } from '../lib/errors';
import { BUG_ICON } from './icons';

const OS_LABEL: Record<BugReportInfo['os'], string> = { linux: 'Linux', windows: 'Windows', macos: 'macOS' };

export function BugReportDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const { container } = useRuntimeEnvironment();
  const [formLink, setFormLink] = useState<{ url: string; lang: string; withLog: boolean } | null>(null);
  const { language } = useLanguage();
  const [info, setInfo] = useState<BugReportInfo | null>(null);
  const [choice, setChoice] = useState<'yes' | 'no' | null>(null);
  const [preview, setPreview] = useState<LogPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const dialogRef = useModalDialog({ open: true, onClose, busy: submitting });
  const mountedRef = useRef(true);
  // Monotonic per-dialog request id: only the reply matching the request
  // that is currently in flight may ever update `preview`, so a slow reply
  // to a superseded "replace file names" toggle can't overwrite a newer one.
  const requestIdRef = useRef(0);

  useEffect(
    () => () => {
      mountedRef.current = false;
    },
    [],
  );

  useEffect(() => {
    getBugReportInfo()
      .then((i) => mountedRef.current && setInfo(i))
      .catch(() => {});
  }, []);

  const formReady = choice === 'no' || (choice === 'yes' && !!preview && !loading && (preview.empty || !!savedPath));
  const withLog = choice === 'yes' && !!savedPath;
  useEffect(() => {
    if (!container || !formReady) return;
    let active = true;
    getBugReportUrl(language, withLog).then(url => {
      if (active) { setFormLink({ url, lang: language, withLog }); setFailure(null); }
    }).catch(e => { if (active) setFailure(messageOf(e)); });
    return () => { active = false; };
  }, [container, formReady, language, withLog]);

  const loadPreview = (replace?: boolean) => {
    const requestId = ++requestIdRef.current;
    setLoading(true);
    previewLogExport(replace)
      .then((p) => {
        if (!mountedRef.current || requestId !== requestIdRef.current) return;
        setPreview(p);
      })
      .catch((e) => {
        if (!mountedRef.current || requestId !== requestIdRef.current) return;
        setFailure(messageOf(e));
      })
      .finally(() => {
        if (!mountedRef.current || requestId !== requestIdRef.current) return;
        setLoading(false);
      });
  };

  const choose = (c: 'yes' | 'no') => {
    setChoice(c);
    if (c === 'yes' && !preview) loadPreview(undefined);
  };

  // Retries opening the report form after the log was already saved but the
  // form itself failed to open (e.g. the browser didn't launch): the main
  // button is hidden once `savedPath` is set, so this is the only way back.
  const retryOpenForm = async () => {
    if (submitting) return;
    setSubmitting(true);
    try {
      await openBugReportForm(language, true);
      if (mountedRef.current) setFailure(null);
    } catch (e) {
      if (mountedRef.current) setFailure(messageOf(e));
    } finally {
      if (mountedRef.current) setSubmitting(false);
    }
  };

  const submit = async () => {
    if (submitting) return;
    setSubmitting(true);
    try {
      if (choice === 'yes' && preview && !preview.empty) {
        const path = await saveLogExport(preview.id);
        // The mounted check must only guard setState, never skip opening the
        // form: once the log is saved, the browser tab has to open even if
        // Escape unmounted the dialog while the save was in flight.
        if (mountedRef.current) setSavedPath(path);
        if (!container) await openBugReportForm(language, true);
      } else {
        if (!container) {
          await openBugReportForm(language, false);
          if (mountedRef.current) onClose();
        }
      }
    } catch (e) {
      if (mountedRef.current) setFailure(messageOf(e));
    } finally {
      if (mountedRef.current) setSubmitting(false);
    }
  };

  const optionClass = (on: boolean) =>
    `flex-1 flex items-center gap-2 px-3 py-2 rounded-[4px] border text-small cursor-pointer ${
      on ? 'border-[var(--accent)] text-[var(--ink)]' : 'border-[var(--line-strong)] text-[var(--ink-2)]'
    }`;

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="bug-report-title"
        tabIndex={-1}
        className="w-[620px] max-w-[92vw] max-h-[90vh] overflow-auto rounded-[6px] border border-[var(--line)] bg-[var(--panel)] shadow-[var(--shadow)] p-5 text-[var(--ink)] outline-0"
      >
        <h2 id="bug-report-title" className="flex items-center gap-2 text-title font-bold">
          <span className="text-[var(--accent)]">{BUG_ICON}</span>
          {t('bugReportTitle')}
        </h2>
        {info && (
          <p className="mt-1 text-small text-[var(--ink-2)]">
            {t('bugReportIntro').replace('{version}', info.version).replace('{os}', OS_LABEL[info.os])}
          </p>
        )}
        <fieldset className="mt-4 p-3 rounded-[5px] border border-[var(--line)] bg-[var(--panel-2)]">
          <legend className="sr-only">{t('bugReportLogQuestion')}</legend>
          <div className="text-body font-semibold">{t('bugReportLogQuestion')}</div>
          <p className="mt-1 text-small text-[var(--ink-2)]">{t('bugReportLogExplain')}</p>
          <div className="mt-2.5 flex gap-2">
            <label className={optionClass(choice === 'yes')}>
              <input type="radio" name="bug-log" checked={choice === 'yes'} onChange={() => choose('yes')} className="accent-[var(--accent)]" />
              {t('bugReportLogYes')}
            </label>
            <label className={optionClass(choice === 'no')}>
              <input type="radio" name="bug-log" checked={choice === 'no'} onChange={() => choose('no')} className="accent-[var(--accent)]" />
              {t('bugReportLogNo')}
            </label>
          </div>
        </fieldset>

        {choice === 'yes' && (
          <div className="mt-3">
            {loading && <div className="text-small text-[var(--ink-3)]">{t('bugReportLoading')}</div>}
            {preview?.empty && <div className="text-small text-[var(--ink-3)]">{t('bugReportEmptyLog')}</div>}
            {preview && !preview.empty && (
              <>
                <pre className="h-[200px] overflow-auto p-2.5 rounded-[5px] border border-[var(--line)] bg-[var(--bg)] font-code text-caption leading-[1.55] text-[var(--ink-2)] whitespace-pre">
                  {preview.segments.map((s, i) =>
                    s.replaced ? (
                      <mark key={i} className="bg-[var(--accent-soft)] text-[var(--accent)] rounded-[2px] px-[1px]">{s.text}</mark>
                    ) : (
                      <span key={i}>{s.text}</span>
                    ),
                  )}
                </pre>
                <label className="mt-2 flex items-center gap-2 text-small text-[var(--ink-2)] cursor-pointer">
                  <input
                    type="checkbox"
                    checked={preview.replaceFileNames}
                    onChange={(e) => loadPreview(e.target.checked)}
                    className="accent-[var(--accent)]"
                  />
                  {t('bugReportReplaceFileNames')}
                </label>
              </>
            )}
          </div>
        )}

        {savedPath && (
          <div className="mt-3 p-2.5 rounded-[5px] border border-[var(--good)] bg-[var(--good-soft)] text-small">
            {t('bugReportSavedTo').replace('{path}', savedPath)}
          </div>
        )}
        {failure && <div className="mt-3 text-small text-[var(--crit)]">{failure}</div>}

        {container && formReady && formLink?.lang === language && formLink.withLog === withLog && (
          <ExternalLink url={formLink.url} label={t('bugReportOpenForm')} />
        )}
        <div className="mt-4 flex items-center justify-between gap-3">
          <span className="text-caption text-[var(--ink-3)]">{choice === 'yes' ? t('bugReportOriginalsUnchanged') : ''}</span>
          <div className="flex gap-2">
            <button
              type="button"
              disabled={submitting}
              onClick={onClose}
              className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] text-small text-[var(--ink-2)] cursor-pointer bg-transparent disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {savedPath ? t('bugReportClose') : t('bugReportCancel')}
            </button>
            {!savedPath && (!container || (choice === 'yes' && preview && !preview.empty)) && (
              <button
                type="button"
                disabled={submitting || choice === null || (choice === 'yes' && (loading || !preview))}
                onClick={submit}
                className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {choice === 'yes' && preview && !preview.empty ? t(container ? 'bugReportSaveLog' : 'bugReportSaveAndOpen') : t('bugReportOpenForm')}
              </button>
            )}
            {!container && savedPath && failure && (
              <button
                type="button"
                disabled={submitting}
                onClick={retryOpenForm}
                className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {t('bugReportOpenForm')}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
