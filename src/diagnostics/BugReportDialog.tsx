import { useEffect, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import {
  getBugReportInfo,
  openBugReportForm,
  previewLogExport,
  saveLogExport,
  type BugReportInfo,
  type LogPreview,
} from '../lib/api/diagnostics';
import { messageOf } from '../lib/errors';

const OS_LABEL: Record<BugReportInfo['os'], string> = { linux: 'Linux', windows: 'Windows', macos: 'macOS' };

export function BugReportDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const { language } = useLanguage();
  const [info, setInfo] = useState<BugReportInfo | null>(null);
  const [choice, setChoice] = useState<'yes' | 'no' | null>(null);
  const [preview, setPreview] = useState<LogPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    getBugReportInfo().then(setInfo).catch(() => {});
  }, []);

  const loadPreview = (replace?: boolean) => {
    setLoading(true);
    previewLogExport(replace)
      .then(setPreview)
      .catch((e) => setFailure(messageOf(e)))
      .finally(() => setLoading(false));
  };

  const choose = (c: 'yes' | 'no') => {
    setChoice(c);
    if (c === 'yes' && !preview) loadPreview(undefined);
  };

  const submit = async () => {
    try {
      if (choice === 'yes' && preview && !preview.empty) {
        setSavedPath(await saveLogExport());
        await openBugReportForm(language, true);
      } else {
        await openBugReportForm(language, false);
        onClose();
      }
    } catch (e) {
      setFailure(messageOf(e));
    }
  };

  const optionClass = (on: boolean) =>
    `flex-1 flex items-center gap-2 px-3 py-2 rounded-[4px] border text-[12.5px] cursor-pointer ${
      on ? 'border-[var(--accent)] text-[var(--ink)]' : 'border-[var(--line-strong)] text-[var(--ink-2)]'
    }`;

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="bug-report-title"
        tabIndex={-1}
        onKeyDown={(e) => e.key === 'Escape' && onClose()}
        className="w-[620px] max-w-[92vw] max-h-[90vh] overflow-auto rounded-[6px] border border-[var(--line)] bg-[var(--panel)] shadow-[var(--shadow)] p-5 text-[var(--ink)] outline-0"
      >
        <h2 id="bug-report-title" className="text-[16px] font-bold">{t('bugReportTitle')}</h2>
        {info && (
          <p className="mt-1 text-[12.5px] text-[var(--ink-2)]">
            {t('bugReportIntro').replace('{version}', info.version).replace('{os}', OS_LABEL[info.os])}
          </p>
        )}
        <fieldset className="mt-4 p-3 rounded-[5px] border border-[var(--line)] bg-[var(--panel-2)]">
          <legend className="sr-only">{t('bugReportLogQuestion')}</legend>
          <div className="text-[13px] font-semibold">{t('bugReportLogQuestion')}</div>
          <p className="mt-1 text-[12px] text-[var(--ink-2)]">{t('bugReportLogExplain')}</p>
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
            {loading && <div className="text-[12px] text-[var(--ink-3)]">{t('bugReportLoading')}</div>}
            {preview?.empty && <div className="text-[12px] text-[var(--ink-3)]">{t('bugReportEmptyLog')}</div>}
            {preview && !preview.empty && (
              <>
                <pre className="h-[200px] overflow-auto p-2.5 rounded-[5px] border border-[var(--line)] bg-[var(--bg)] font-mono-ui text-[10.5px] leading-[1.55] text-[var(--ink-2)] whitespace-pre">
                  {preview.segments.map((s, i) =>
                    s.replaced ? (
                      <mark key={i} className="bg-[var(--accent-soft)] text-[var(--accent)] rounded-[2px] px-[1px]">{s.text}</mark>
                    ) : (
                      <span key={i}>{s.text}</span>
                    ),
                  )}
                </pre>
                <label className="mt-2 flex items-center gap-2 text-[12.5px] text-[var(--ink-2)] cursor-pointer">
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
          <div className="mt-3 p-2.5 rounded-[5px] border border-[var(--good)] bg-[var(--good-soft)] text-[12px]">
            {t('bugReportSavedTo').replace('{path}', savedPath)}
          </div>
        )}
        {failure && <div className="mt-3 text-[12px] text-[var(--crit)]">{failure}</div>}

        <div className="mt-4 flex items-center justify-between gap-3">
          <span className="text-[11px] text-[var(--ink-3)]">{choice === 'yes' ? t('bugReportOriginalsUnchanged') : ''}</span>
          <div className="flex gap-2">
            <button type="button" onClick={onClose} className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] text-[12.5px] text-[var(--ink-2)] cursor-pointer bg-transparent">
              {savedPath ? t('bugReportClose') : t('bugReportCancel')}
            </button>
            {!savedPath && (
              <button
                type="button"
                disabled={choice === null || (choice === 'yes' && (loading || !preview))}
                onClick={submit}
                className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {choice === 'yes' && preview && !preview.empty ? t('bugReportSaveAndOpen') : t('bugReportOpenForm')}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
