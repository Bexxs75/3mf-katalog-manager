import { useEffect, useRef, useState } from 'react';
import { useT, useFormatCount, useLanguage } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { toAppError, type AppError } from '../lib/errors';
import { displayName } from '../lib/displayName';
import { formatBytes } from '../i18n/format';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';
import type { SourceScan } from '../types';

const primary = `${catalogActionBase} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)]`;
const focusRing = 'focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

// Copies of models already in the catalog are opt-in; everything else starts ticked.
const defaultTicked = (scans: SourceScan[]) => scans.flatMap(s => s.files).filter(f => f.duplicateOfId === null).map(f => f.path);

interface Props {
  scans: SourceScan[];
  onClose: () => void;
  /** Starts the move into the catalog; rejects with the backend error. */
  onImport: (paths: string[]) => Promise<unknown>;
  /** Remembers the files as declined and returns how many. */
  onReject: (paths: string[]) => Promise<number>;
  /** Looks at the folders again after files were declined. */
  onRescan: () => Promise<unknown>;
}

export function SourceFilesDialog({ scans, onClose, onImport, onReject, onRescan }: Props) {
  const t = useT();
  const count = useFormatCount();
  const { language } = useLanguage();
  const { jobActive } = useImportLock();
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set(defaultTicked(scans)));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [rejectedNote, setRejectedNote] = useState<number | null>(null);
  const noteRef = useRef<HTMLParagraphElement>(null);
  const known = useRef(new Set(scans.flatMap(s => s.files.map(f => f.path))));
  const returnFocus = useRef<HTMLElement | null>(document.querySelector<HTMLElement>('[data-catalog-main]'));

  // After a rescan keep the user's ticks for files that remain; only files not seen before get the default.
  useEffect(() => {
    const defaults = new Set(defaultTicked(scans));
    setSelected(previous => {
      const next = new Set<string>();
      for (const file of scans.flatMap(s => s.files)) {
        if (known.current.has(file.path) ? previous.has(file.path) : defaults.has(file.path)) next.add(file.path);
      }
      return next;
    });
    for (const file of scans.flatMap(s => s.files)) known.current.add(file.path);
  }, [scans]);

  const chosen = scans.flatMap(s => s.files).map(f => f.path).filter(path => selected.has(path));
  const toggle = (path: string, on: boolean) => setSelected(previous => {
    const next = new Set(previous);
    if (on) next.add(path); else next.delete(path);
    return next;
  });

  const move = async () => {
    if (busy || chosen.length === 0) return;
    setBusy(true); setError(null);
    try { await onImport(chosen); onClose(); }
    catch (e) { setError(toAppError(e)); setBusy(false); }
  };
  const reject = async () => {
    if (busy || chosen.length === 0) return;
    setBusy(true); setError(null);
    try {
      const done = await onReject(chosen);
      setRejectedNote(done);
      await onRescan();
      // The ticked rows are gone; the note is a stable place for focus and is announced.
      noteRef.current?.focus();
    } catch (e) { setError(toAppError(e)); }
    finally { setBusy(false); }
  };

  const hasDuplicates = scans.some(s => s.files.some(f => f.duplicateOfId !== null));
  const shown = scans.filter(s => s.files.length > 0 || s.unreachable);
  let first = true;
  return (
    <CatalogActionDialog title={t('sfDialogTitle')} onClose={onClose} busy={busy} wide returnFocus={returnFocus}>
      <p ref={noteRef} role="status" tabIndex={-1} className="text-small text-[var(--ink-2)] empty:hidden">
        {rejectedNote !== null && count(t('sfRejectedDone'), rejectedNote)}
      </p>
      {shown.map(scan => (
        <section key={scan.folderId} aria-label={displayName(scan.folderPath)}>
          <h3 className="text-small font-semibold font-code [overflow-wrap:anywhere] mb-1">{displayName(scan.folderPath)}</h3>
          {scan.unreachable && <p className="text-small text-[var(--crit)] mb-1">{t('sfUnreachable')}</p>}
          {scan.truncated && <p className="text-small text-[var(--ink-2)] mb-1">{t('sfTruncated')}</p>}
          {scan.skipped > 0 && <p className="text-small text-[var(--ink-2)] mb-1">{count(t('sfSkippedEntries'), scan.skipped)}</p>}
          <ul className="space-y-1">
            {scan.files.map(file => {
              const focusFirst = first; first = false;
              const noteId = `sf-dup-${file.path}`;
              return (
                <li key={file.path} className="rounded bg-[var(--panel-2)] p-2 text-small">
                  <label className="flex items-start gap-2 cursor-pointer">
                    <input type="checkbox" className={`mt-0.5 shrink-0 ${focusRing}`} {...(focusFirst ? { 'data-initial-focus': true } : {})}
                      aria-label={displayName(file.relativePath)} aria-describedby={file.duplicateOfId !== null ? noteId : undefined}
                      checked={selected.has(file.path)} disabled={busy} onChange={e => toggle(file.path, e.target.checked)} />
                    <span className="min-w-0 flex-1">
                      <span className="block font-code [overflow-wrap:anywhere]">{displayName(file.relativePath)}</span>
                      {file.duplicateOfId !== null && <span id={noteId} className="mt-1 inline-block rounded-full px-2 py-0.5 bg-[var(--accent-soft)] text-[var(--accent)] font-semibold">{t('sfDuplicate')}</span>}
                    </span>
                    <span className="shrink-0 text-[var(--ink-3)]"><span className="sr-only">{t('sfSizeLabel')}: </span>{formatBytes(file.sizeBytes, language)}</span>
                  </label>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
      {hasDuplicates && <p className="text-small text-[var(--ink-2)]">{t('sfDuplicateHint')}</p>}
      {error && <p role="alert" className="text-small text-[var(--crit)] whitespace-pre-wrap [overflow-wrap:anywhere]"><ErrorText error={error} /></p>}
      <div className="flex flex-wrap justify-end gap-2">
        <button type="button" className={`${catalogActionButton} ${focusRing}`} disabled={busy || chosen.length === 0} onClick={() => void reject()}>{t('sfReject')}</button>
        <button type="button" className={`${catalogActionButton} ${focusRing}`} disabled={busy} onClick={onClose}>{t('sfLater')}</button>
        <button type="button" className={`${primary} ${focusRing}`} disabled={busy || chosen.length === 0 || jobActive} title={jobActive ? t('importLockedHint') : undefined} onClick={() => void move()}>
          {busy ? t('sfWorking') : count(t('sfMove'), chosen.length)}
        </button>
      </div>
      <p className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{t('sfDialogHint')}</p>
    </CatalogActionDialog>
  );
}
