import { useEffect, useRef } from 'react';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { displayName } from '../lib/displayName';
import type { AppError } from '../lib/errors';
import errorCodes from '../lib/errorCodes.json';
import { sourceImportFinished, type SourceImportJob } from '../lib/api/sourceImport';
import { catalogActionBase, catalogActionButton } from './CatalogActionDialog';

const primary = `${catalogActionBase} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)]`;
const focusRing = 'focus-visible:outline-2 focus-visible:outline-[var(--accent)]';
const box = 'rounded border px-3 py-2 text-small';
const REGISTERED: ReadonlySet<string> = new Set(errorCodes.registered);
// Maximum number of unreadable file names listed; the count above them stays exact.
const MAX_LISTED = 5;

interface Props {
  job: SourceImportJob;
  /** The job could not be followed any more (connection lost); the copy may still run on the server. */
  error: AppError | null;
  /** Name of the copied folder, for the "originals unchanged" sentence. */
  sourceName: string;
  onCancel: () => void;
  onClose: () => void;
}

/** Copy progress and result of one source import, shared by the folder picker and the new-files dialog. */
export function SourceImportProgress({ job, error, sourceName, onCancel, onClose }: Props) {
  const t = useT();
  const count = useFormatCount();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const ended = error !== null || sourceImportFinished(job);
  const cancelling = job.state === 'cancelling';

  // The button that started the copy is gone; keep focus inside the dialog on the next useful control.
  useEffect(() => { (ended ? closeRef : cancelRef).current?.focus(); }, [ended]);

  const percent = job.total > 0 ? Math.min(100, Math.round(job.done / job.total * 100)) : 0;
  const imported = job.files.filter(f => f.state === 'imported').length;
  const duplicates = job.files.filter(f => f.state === 'duplicate').length;
  const invalid = job.files.filter(f => f.state === 'invalid');
  const originals = t('sfiDoneKept').replace('{name}', displayName(sourceName));
  const fill = (text: string) => text.replace('{done}', String(job.done)).replace('{total}', String(job.total));
  // A code the client does not know (internal failure, restart needed) gets a plain sentence instead of the raw word.
  const failure: AppError | null = job.state === 'failed'
    ? { message: job.kind && REGISTERED.has(job.kind) ? job.kind : t(job.kind === 'restart_required' ? 'webUploadRestart' : 'webUploadFailed'), unexpected: false }
    : null;

  return (
    <>
      <div className={`${box} border-[var(--line)] bg-[var(--panel-2)]`}>{t('sfpCopyNote')}</div>
      {!ended && (
        <div aria-live="polite" aria-atomic="true">
          <p className="text-small font-semibold tabular-nums">
            {cancelling ? t('sfiStopping') : job.total === 0 ? t('sfiPreparing') : fill(t('sfiProgress'))}
          </p>
          {job.current && !cancelling && <p className="text-small text-[var(--ink-2)] [overflow-wrap:anywhere]">{t('sfiCurrent').replace('{name}', displayName(job.current))}</p>}
        </div>
      )}
      {!ended && (
        <div role="progressbar" aria-label={t('sfiBarLabel')} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}
          className="h-2 overflow-hidden rounded-full bg-[var(--line)]">
          <div className="h-full bg-[var(--accent)]" style={{ width: `${percent}%` }} />
        </div>
      )}
      {error && <p role="alert" className="text-small text-[var(--crit)] whitespace-pre-wrap [overflow-wrap:anywhere]"><ErrorText error={error} /></p>}
      {!error && job.state === 'completed' && (
        <div role="status" className={`${box} border-[var(--good)] bg-[var(--good-soft)]`}>
          <b className="font-semibold">{t('sfiDoneTitle')}</b> {count(t('sfiDoneCopied'), imported)}
          {duplicates > 0 && <> {count(t('sfiDoneSkipped'), duplicates)}</>} {originals}
        </div>
      )}
      {!error && job.state === 'cancelled' && (
        <div role="status" className={`${box} border-[var(--warn)] bg-[var(--warn-soft)]`}>
          <b className="font-semibold">{t('sfiCancelledTitle')}</b> {fill(t('sfiCancelledText'))} {originals}
        </div>
      )}
      {!error && failure && (
        <div role="alert" className={`${box} border-[var(--crit)] bg-[var(--crit-soft)]`}>
          <b className="font-semibold">{t('sfiFailedTitle')}</b>{' '}
          <span className="whitespace-pre-wrap [overflow-wrap:anywhere]"><ErrorText error={failure} /></span>{' '}
          {t('sfiFailedKept')}
        </div>
      )}
      {ended && invalid.length > 0 && (
        <div className="text-small">
          <p>{count(t('sfiDoneInvalid'), invalid.length)}</p>
          <ul className="list-disc pl-5 font-code [overflow-wrap:anywhere]">
            {invalid.slice(0, MAX_LISTED).map(f => <li key={f.uploadId}>{displayName(f.name)}</li>)}
          </ul>
        </div>
      )}
      <div className="flex flex-wrap justify-end gap-2">
        {ended
          ? <button ref={closeRef} type="button" className={`${primary} ${focusRing}`} onClick={onClose}>{t('sfpClose')}</button>
          // aria-disabled instead of disabled: a disabled button would drop the focus the user just had on it.
          : <button ref={cancelRef} type="button" className={`${catalogActionButton} ${focusRing}`} aria-disabled={cancelling || undefined}
            onClick={() => { if (!cancelling) onCancel(); }}>{cancelling ? t('sfiStopping') : t('sfiStop')}</button>}
      </div>
    </>
  );
}
