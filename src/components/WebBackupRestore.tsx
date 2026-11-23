import { useCallback, useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes } from '../i18n/format';
import { displayName } from '../lib/displayName';
import { toAppError, type AppError } from '../lib/errors';
import { useCanEditMetadata } from '../lib/webSession';
import { BACKUP_MAX_BYTES, backupFileProblem, discardBackup, restoreBackup, uploadBackup, waitForServer } from '../lib/api/backupRestore';
import { ErrorText } from '../diagnostics/ErrorText';
import type { BackupPreview } from '../types';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';

type Step = 'file' | 'password' | 'report' | 'confirm' | 'working' | 'result';
const STEP_ORDER: readonly Step[] = ['file', 'password', 'report', 'confirm', 'working', 'result'];
const STEP_NAME_KEYS = ['bkrStepFile', 'bkrStepPassword', 'bkrStepReport', 'bkrStepConfirm', 'bkrStepProgress', 'bkrStepResult'] as const;
// Longest warning list shown before "and n more".
const LIST_LIMIT = 5;
// A refusal after which the checked backup is still on the server, so asking again is sensible.
const RETRYABLE = new Set(['restoreDrainTimeout', 'restoreJournalPending', 'restoreUploadsPending', 'maintenance', 'webTooManyRequests', 'webTooManyAttempts']);
// Statuses a proxy answers when it gave up waiting: the server may well have finished.
const LOST_ANSWER_STATUSES = [408, 499, 502, 504, 524];

type Outcome =
  | { kind: 'done'; waiting: 'waiting' | 'up' | 'slow'; unknown: boolean }
  | { kind: 'failed'; error: AppError; keepToken: boolean };

const dashedButton = 'block w-full text-center leading-7 h-7 rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small';
const primaryButton = `${catalogActionBase} border-[var(--accent)] bg-[var(--accent)] text-white`;
const dangerButton = `${catalogActionBase} border-[var(--crit)] bg-[var(--crit)] text-white`;
const fieldClass = 'block w-full h-8 px-2 border border-[var(--line-strong)] rounded-[3px] bg-[var(--panel-2)] text-[var(--ink)]';
const boxClass = 'rounded-[3px] border border-[var(--line)] bg-[var(--panel-2)] p-3 text-small';

function lastSegment(path: string): string {
  return displayName(path.split(/[\\/]/).filter(Boolean).pop() ?? path);
}

/** Entry point in the web settings: the button, or its explanation when the login may only look. */
export function WebBackupRestore() {
  const t = useT();
  const canWrite = useCanEditMetadata();
  const [open, setOpen] = useState(false);
  const opener = useRef<HTMLButtonElement>(null);
  const hintId = useId();
  return (
    <>
      {/* aria-disabled instead of disabled keeps the button reachable, so its reason is read out. */}
      <button
        ref={opener}
        type="button"
        aria-disabled={canWrite ? undefined : 'true'}
        aria-describedby={hintId}
        onClick={() => { if (canWrite) setOpen(true); }}
        className={`${dashedButton} ${canWrite ? 'cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]' : 'opacity-50 cursor-not-allowed'} focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]`}
      >
        {t('bkrOpen')}
      </button>
      <p id={hintId} className="mt-1.5 text-small text-[var(--ink-2)]">{canWrite ? t('bkrOpenHint') : t('bkrReadOnly')}</p>
      {open && canWrite && <RestoreDialog returnFocus={opener} onClose={() => setOpen(false)} />}
    </>
  );
}

function RestoreDialog({ returnFocus, onClose }: { returnFocus: React.RefObject<HTMLElement | null>; onClose: () => void }) {
  const t = useT();
  const { language } = useLanguage();
  const [step, setStep] = useState<Step>('file');
  const [file, setFile] = useState<File | null>(null);
  const [fileProblem, setFileProblem] = useState('');
  const [password, setPassword] = useState('');
  const [fieldError, setFieldError] = useState('');
  const [error, setError] = useState<AppError | null>(null);
  const [percent, setPercent] = useState<number | null>(null);
  const [preview, setPreview] = useState<BackupPreview | null>(null);
  const [listOpen, setListOpen] = useState(false);
  const [acknowledged, setAcknowledged] = useState(false);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const passwordInput = useRef<HTMLInputElement>(null);
  const loginButton = useRef<HTMLButtonElement>(null);
  const upload = useRef<AbortController | null>(null);
  const watcher = useRef<AbortController | null>(null);
  // The server-side copy of a checked backup; cleared once it has been used or removed.
  const token = useRef<string | null>(null);
  const errorId = useId();
  const warningListId = useId();

  const uploading = percent !== null;
  const working = step === 'working';
  const stepNumber = STEP_ORDER.indexOf(step) + 1;
  const stepName = t(STEP_NAME_KEYS[stepNumber - 1]);
  const fail = useCallback((value: unknown) => setError(toAppError(value)), []);

  // A new step starts at its heading so a screen reader announces it; a finished restore leads on to the sign-in.
  useEffect(() => {
    if (step === 'result' && outcome?.kind === 'done' && outcome.waiting !== 'waiting') loginButton.current?.focus();
    else headingRef.current?.focus();
    // Only a change of step moves the focus; later updates of the outcome must not steal it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step]);
  // A rejected password leaves the cursor in the field it concerns.
  useEffect(() => { if ((error || fieldError) && !uploading) passwordInput.current?.focus(); }, [error, fieldError, uploading]);
  // When the wait for the restarted server ends, the sign-in button takes the focus.
  const waitingState = outcome?.kind === 'done' ? outcome.waiting : null;
  useEffect(() => {
    if (waitingState === 'up' || waitingState === 'slow') loginButton.current?.focus();
  }, [waitingState]);
  useEffect(() => () => { upload.current?.abort(); watcher.current?.abort(); }, []);

  const dropToken = () => {
    const held = token.current;
    token.current = null;
    if (held) void discardBackup(held);
  };
  const close = () => {
    if (working) return;
    upload.current?.abort();
    dropToken();
    onClose();
  };
  const startOver = () => {
    dropToken();
    setPreview(null); setFile(null); setFileProblem(''); setError(null); setFieldError(''); setPassword('');
    setAcknowledged(false); setListOpen(false); setOutcome(null); setPercent(null);
    setStep('file');
  };

  const choose = (picked: File | undefined) => {
    if (!picked) return;
    setError(null);
    const problem = backupFileProblem(picked);
    const text = problem === 'not_zip' ? t('bkrFileNotZip')
      : problem === 'empty' ? t('bkrFileEmpty')
      : problem === 'too_large' ? t('bkrFileTooLarge') : '';
    setFile(picked);
    setFileProblem(text.replace('{name}', displayName(picked.name)).replace('{size}', formatBytes(picked.size, language)).replace('{max}', formatBytes(BACKUP_MAX_BYTES, language)));
  };

  const submitPassword = async (event: FormEvent) => {
    event.preventDefault();
    if (!file || uploading) return;
    if (!password) { setFieldError(t('bkrPwEmpty')); return; }
    // The secret leaves the state before the request starts; only this call still holds it.
    const secret = password;
    setPassword(''); setFieldError(''); setError(null); setPercent(0);
    const controller = new AbortController();
    upload.current = controller;
    try {
      const result = await uploadBackup(file, secret, setPercent, controller.signal);
      if (controller.signal.aborted) { void discardBackup(result.token); return; }
      token.current = result.token;
      setPreview(result); setListOpen(false); setStep('report');
    } catch (e) {
      if (!controller.signal.aborted) fail(e);
    } finally {
      upload.current = null;
      setPercent(null);
    }
  };

  const watchServer = (unknown: boolean) => {
    setOutcome({ kind: 'done', waiting: 'waiting', unknown });
    watcher.current?.abort();
    const controller = new AbortController();
    watcher.current = controller;
    void waitForServer({ signal: controller.signal }).then((up) => {
      if (!controller.signal.aborted) setOutcome({ kind: 'done', waiting: up ? 'up' : 'slow', unknown });
    });
  };

  const submitRestore = async (event: FormEvent) => {
    event.preventDefault();
    const held = token.current;
    if (!held || working) return;
    if (!acknowledged) return;
    if (!password) { setFieldError(t('bkrPwEmpty')); return; }
    const secret = password;
    setPassword(''); setFieldError(''); setError(null);
    setStep('working');
    try {
      await restoreBackup(held, secret);
      token.current = null;
      setStep('result');
      watchServer(false);
    } catch (e) {
      const failure = e as { message?: string; status?: number };
      if (failure.message === 'backupConnectionLost' || LOST_ANSWER_STATUSES.includes(failure.status ?? 0)) {
        // No answer says nothing about the outcome; the restart may be under way.
        token.current = null;
        setStep('result');
        watchServer(true);
      } else if (failure.message === 'password_invalid') {
        fail(e); setStep('confirm');
      } else if (failure.message === 'webNotFound') {
        token.current = null; setPreview(null);
        setError({ message: 'backupTokenGone', unexpected: false }); setStep('file');
      } else {
        setOutcome({ kind: 'failed', error: toAppError(e), keepToken: RETRYABLE.has(failure.message ?? '') });
        setStep('result');
      }
    }
  };

  const stepList = (
    <div className="space-y-1.5">
      <p className="text-small text-[var(--ink-2)]">
        <span className="tabular-nums">{t('bkrStepOf').replace('{n}', String(stepNumber)).replace('{total}', String(STEP_ORDER.length))}</span> · {stepName}
      </p>
      <ol aria-label={t('bkrStepsLabel')} className="flex gap-1 list-none p-0 m-0">
        {STEP_NAME_KEYS.map((key, index) => (
          <li key={key} aria-current={index + 1 === stepNumber ? 'step' : undefined}
            className={`h-1 flex-1 rounded-sm ${index + 1 <= stepNumber ? 'bg-[var(--accent)]' : 'bg-[var(--line)]'}`}>
            <span className="sr-only">{index + 1 < stepNumber ? t('bkrStepDone').replace('{name}', t(key)) : t(key)}</span>
          </li>
        ))}
      </ol>
    </div>
  );

  const heading = (text: string, hint?: string): ReactNode => (
    <div>
      <h3 ref={headingRef} tabIndex={-1} data-initial-focus className="text-body font-semibold outline-none">{text}</h3>
      {hint && <p className="mt-1 text-small text-[var(--ink-2)]">{hint}</p>}
    </div>
  );
  const errorBox = (value: AppError | null, text?: string) => (value || text) && (
    <div id={errorId} role="alert" className="text-small text-[var(--crit)] break-words">{value ? <ErrorText error={value} /> : text}</div>
  );
  const passwordField = (
    <div>
      <label htmlFor={`${errorId}-pw`} className="block text-small mb-1">{t('bkrPwLabel')}</label>
      <input
        id={`${errorId}-pw`} ref={passwordInput} type="password" autoComplete="current-password" value={password}
        disabled={uploading}
        aria-invalid={error || fieldError ? 'true' : undefined} aria-describedby={error || fieldError ? errorId : undefined}
        onChange={(e) => { setPassword(e.target.value); if (fieldError) setFieldError(''); }}
        className={`${fieldClass} focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]`}
      />
      {errorBox(error, fieldError)}
    </div>
  );

  let body: ReactNode = null;
  if (step === 'file') {
    body = (
      <div className="space-y-3">
        {heading(t('bkrFileTitle'), t('bkrFileHint').replace('{max}', formatBytes(BACKUP_MAX_BYTES, language)))}
        <input ref={fileInput} type="file" accept=".zip,application/zip" tabIndex={-1} aria-hidden="true" className="sr-only"
          onChange={(e) => { const picked = e.currentTarget.files?.[0]; e.currentTarget.value = ''; choose(picked); }} />
        <button type="button" className={catalogActionButton} onClick={() => fileInput.current?.click()}>{t('bkrChoose')}</button>
        <div aria-live="polite" className="space-y-2">
          {file && !fileProblem && <p className={boxClass}><span className="font-mono break-all">{displayName(file.name)}</span><br /><span className="tabular-nums text-[var(--ink-2)]">{formatBytes(file.size, language)}</span></p>}
          {fileProblem && <p role="alert" className="text-small text-[var(--crit)] break-words">{fileProblem}</p>}
          {error && <p role="alert" className="text-small text-[var(--crit)] break-words"><ErrorText error={error} /></p>}
        </div>
        <p className={boxClass}>{t('bkrFileInfo')}</p>
        <div className="flex justify-end gap-2">
          <button type="button" className={catalogActionButton} onClick={close}>{t('cancel')}</button>
          <button type="button" className={primaryButton} disabled={!file || !!fileProblem} onClick={() => { setError(null); setStep('password'); }}>{t('bkrNext')}</button>
        </div>
      </div>
    );
  } else if (step === 'password') {
    body = (
      <form className="space-y-3" noValidate onSubmit={submitPassword}>
        {heading(t('bkrPwTitle'), t('bkrPwHint'))}
        {passwordField}
        {percent !== null && (
          <div role="status" className={boxClass}>
            <div>{percent >= 100 ? t('bkrCheckingServer') : t('bkrChecking')}</div>
            {percent < 100 && <div className="tabular-nums text-[var(--ink-2)]">{t('bkrUploadPercent').replace('{percent}', String(percent))}</div>}
            <progress value={percent} max={100} aria-label={t('bkrUploadLabel')} className="block w-full h-1.5 mt-2" />
          </div>
        )}
        <div className="flex justify-end gap-2">
          <button type="button" className={catalogActionButton} disabled={uploading} onClick={() => { setError(null); setFieldError(''); setPassword(''); setStep('file'); }}>{t('bkrBack')}</button>
          <button type="submit" className={primaryButton} disabled={uploading}>{t('bkrCheck')}</button>
        </div>
      </form>
    );
  } else if (step === 'report' && preview) {
    const { report } = preview;
    const warnings = report.path_warnings;
    const shown = warnings.slice(0, LIST_LIMIT);
    const unreachable = report.unreachable_source_folders;
    const number = new Intl.NumberFormat(language);
    body = (
      <div className="space-y-3">
        {heading(t('bkrReportTitle'))}
        <p role="status" className={`${boxClass} ${report.migration_needed ? 'border-[var(--warn,var(--line-strong))]' : ''}`}>
          {(report.migration_needed ? t('bkrReportMigrate') : t('bkrReportClean')).replace('{schema}', String(report.schema_version))}
        </p>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-small">
          <dt className="text-[var(--ink-2)]">{t('bkrFactFile')}</dt><dd className="font-mono break-all">{file ? displayName(file.name) : ''}</dd>
          <dt className="text-[var(--ink-2)]">{t('bkrFactSchema')}</dt><dd className="tabular-nums">{report.schema_version}</dd>
          <dt className="text-[var(--ink-2)]">{t('bkrFactModels')}</dt><dd className="tabular-nums">{number.format(report.model_count)}</dd>
          <dt className="text-[var(--ink-2)]">{t('bkrFactFolders')}</dt><dd className="tabular-nums">{number.format(report.folder_count)}</dd>
        </dl>
        {warnings.length > 0 && (
          <div>
            <button type="button" aria-expanded={listOpen} aria-controls={warningListId} onClick={() => setListOpen((v) => !v)}
              className="text-small underline cursor-pointer text-left focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]">
              {t('bkrWarnToggle').replace('{count}', number.format(warnings.length))}
            </button>
            <div id={warningListId} aria-live="polite">
              {listOpen && (
                <>
                  <ul className="mt-1 text-small font-mono list-disc pl-5 break-all">
                    {shown.map((w, i) => <li key={`${w.table}-${w.column}-${w.row_id}-${i}`}>{displayName(w.display_path)}</li>)}
                    {warnings.length > shown.length && <li className="list-none font-sans">{t('bkrMore').replace('{count}', number.format(warnings.length - shown.length))}</li>}
                  </ul>
                  <p className="mt-1 text-small text-[var(--ink-2)]">{t('bkrWarnNote')}</p>
                </>
              )}
            </div>
          </div>
        )}
        {unreachable.length > 0 && (
          <div className={boxClass}>
            <p>{t('bkrUnreachable').replace('{count}', number.format(unreachable.length))}</p>
            <ul className="mt-1 list-disc pl-5 break-all">
              {unreachable.slice(0, LIST_LIMIT).map((p, i) => <li key={`${p}-${i}`}>{lastSegment(p)}</li>)}
              {unreachable.length > LIST_LIMIT && <li className="list-none">{t('bkrMore').replace('{count}', number.format(unreachable.length - LIST_LIMIT))}</li>}
            </ul>
          </div>
        )}
        {preview.settingsIgnored && <p className={boxClass}>{t('bkrSettingsIgnored')}</p>}
        <div className="flex justify-end gap-2">
          <button type="button" className={catalogActionButton} onClick={startOver}>{t('bkrDiscard')}</button>
          <button type="button" className={primaryButton} onClick={() => { setAcknowledged(false); setError(null); setStep('confirm'); }}>{t('bkrToConfirm')}</button>
        </div>
      </div>
    );
  } else if (step === 'confirm') {
    body = (
      <form className="space-y-3" noValidate onSubmit={submitRestore}>
        {heading(t('bkrConfirmTitle'))}
        <div className="rounded-[3px] border border-[var(--crit)] p-3 text-small space-y-1.5">
          <p className="font-semibold">{t('bkrWarnHeading')}</p>
          <ul className="list-disc pl-5 space-y-1">
            <li>{t('bkrWarn1')}</li><li>{t('bkrWarn2')}</li><li>{t('bkrWarn3')}</li><li>{t('bkrWarn4')}</li>
          </ul>
        </div>
        <label className="flex items-start gap-2 text-small cursor-pointer">
          <input type="checkbox" checked={acknowledged} onChange={(e) => setAcknowledged(e.target.checked)} className="mt-0.5" />
          <span>{t('bkrAck')}</span>
        </label>
        {passwordField}
        <div className="flex justify-end gap-2">
          <button type="button" className={catalogActionButton} onClick={() => { setError(null); setFieldError(''); setPassword(''); setStep('report'); }}>{t('bkrBack')}</button>
          <button type="submit" className={dangerButton} disabled={!acknowledged || !password}>{t('bkrReplace')}</button>
        </div>
      </form>
    );
  } else if (step === 'working') {
    body = (
      <div className="space-y-3">
        {heading(t('bkrWorkTitle'), t('bkrWorkHint'))}
        <div role="status" aria-atomic="true" className={boxClass}>{t('bkrWorking')}</div>
        <progress aria-label={t('bkrProgressLabel')} className="block w-full h-1.5" />
        <p className="text-small text-[var(--ink-2)]">{t('bkrWorkNoCancel')}</p>
      </div>
    );
  } else if (step === 'result' && outcome?.kind === 'done') {
    const { waiting, unknown } = outcome;
    body = (
      <div className="space-y-3">
        {heading(t('bkrDoneTitle'))}
        <p role="status" className={boxClass}>{unknown ? t('bkrUnknownOutcome') : t('bkrDoneText')}</p>
        {!unknown && <p className="text-small text-[var(--ink-2)]">{t('bkrBackupsNote')}</p>}
        <p role="status" className="text-small">
          {waiting === 'waiting' ? t('bkrWaiting') : waiting === 'up' ? t('bkrServerUp') : t('bkrServerSlow')}
          {waiting === 'waiting' && <progress aria-label={t('bkrWaiting')} className="block w-full h-1.5 mt-2" />}
        </p>
        <div className="flex justify-end gap-2">
          {waiting === 'slow' && <button type="button" className={catalogActionButton} onClick={() => watchServer(unknown)}>{t('bkrCheckAgain')}</button>}
          <button ref={loginButton} type="button" className={primaryButton} disabled={waiting === 'waiting'} onClick={() => window.location.reload()}>{t('bkrToLogin')}</button>
        </div>
      </div>
    );
  } else if (step === 'result' && outcome?.kind === 'failed') {
    body = (
      <div className="space-y-3">
        {heading(t('bkrFailTitle'))}
        <div role="alert" className={`${boxClass} border-[var(--crit)] break-words`}><ErrorText error={outcome.error} /></div>
        {outcome.keepToken && <p className="text-small text-[var(--ink-2)]">{t('bkrFailKept')}</p>}
        <div className="flex justify-end gap-2">
          <button type="button" className={catalogActionButton} onClick={close}>{t('bkrClose')}</button>
          {outcome.keepToken && <button type="button" className={catalogActionButton} onClick={() => { setOutcome(null); setError(null); setAcknowledged(false); setStep('confirm'); }}>{t('bkrTryAgain')}</button>}
          <button type="button" className={primaryButton} onClick={startOver}>{t('bkrFailChooseOther')}</button>
        </div>
      </div>
    );
  }

  return (
    <CatalogActionDialog title={t('bkrTitle')} closeLabel={t('bkrClose')} busy={working} returnFocus={returnFocus} onClose={close}>
      {stepList}
      {body}
    </CatalogActionDialog>
  );
}
