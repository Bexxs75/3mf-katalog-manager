import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { toAppError, type AppError } from '../lib/errors';
import { displayName } from '../lib/displayName';
import { pickFolderPath } from '../lib/api/importExport';
import * as api from '../lib/api/sourceFolders';
import type { SourceFolder } from '../types';

const button = 'px-2 py-1 rounded border border-[var(--line)] bg-[var(--panel)] text-small cursor-pointer disabled:opacity-50 disabled:cursor-default hover:border-[var(--accent)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/** Settings section: remembered folders that are checked for new model files. */
export function SourceFoldersSettings() {
  const t = useT();
  const { jobActive, lockProps } = useImportLock();
  const [folders, setFolders] = useState<SourceFolder[]>([]);
  const [checkOnStart, setCheckOnStart] = useState(false);
  const [rejected, setRejected] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const addRef = useRef<HTMLButtonElement>(null);
  const mounted = useRef(true);
  const refocus = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  // The add button is disabled while an action runs, and a disabled button cannot take focus: wait for the re-enable.
  useEffect(() => {
    if (!busy && refocus.current) { refocus.current = false; addRef.current?.focus(); }
  }, [busy]);

  const load = useCallback(async () => {
    const [list, start, count] = await Promise.all([api.listSourceFolders(), api.getSourceCheckOnStart(), api.listRejectedCount()]);
    if (!mounted.current) return;
    setFolders(list); setCheckOnStart(start === true); setRejected(count);
  }, []);
  useEffect(() => { load().catch(e => { if (mounted.current) setError(toAppError(e)); }); }, [load]);

  // One place for the busy flag and error handling; the backend text is shown as is.
  const run = async (action: () => Promise<unknown>, focusAfter = false) => {
    setBusy(true); setError(null);
    try { await action(); refocus.current = focusAfter; }
    catch (e) { if (mounted.current) setError(toAppError(e)); }
    finally { if (mounted.current) setBusy(false); }
  };
  const add = () => run(async () => {
    const path = await pickFolderPath();
    if (!path) return;
    await api.addSourceFolder(path);
    await load();
  });
  const remove = (folder: SourceFolder) => run(async () => {
    await api.removeSourceFolder(folder.id);
    await load();
  // The removed row takes its button along; park focus on the add button instead of losing it.
  }, true);
  const changeCheckOnStart = (enabled: boolean) => run(async () => {
    await api.setSourceCheckOnStart(enabled);
    setCheckOnStart(enabled);
  });
  // The reset button turns disabled at zero; keep focus on a control that stays usable.
  const reset = () => run(async () => {
    await api.resetRejectedSourceFiles();
    await load();
  }, true);

  return (
    <section className="mb-4" aria-labelledby="source-folders-title">
      <h3 id="source-folders-title" className="text-[length:var(--font-size-body)] font-semibold mb-1">{t('sfTitle')}</h3>
      <p className="text-small text-[var(--ink-2)] mb-2">{t('sfDescription')}</p>
      {folders.length === 0
        ? <p className="text-small text-[var(--ink-2)] mb-2">{t('sfNone')}</p>
        : <ul className="mb-2">{folders.map(folder => (
          <li key={folder.id} className="flex items-start gap-2 py-1.5 border-t border-[var(--line)] first:border-t-0">
            <span className="min-w-0 flex-1 text-small font-code [overflow-wrap:anywhere]">
              {displayName(folder.path)}
              {!folder.reachable && <span className="ml-2 font-sans text-[var(--crit)]">({t('sfUnreachable')})</span>}
            </span>
            <button type="button" className={`${button} shrink-0`} disabled={busy || jobActive} {...lockProps}
              aria-label={t('sfRemoveLabel').replace('{path}', displayName(folder.path))} onClick={() => void remove(folder)}>
              {t('sfRemove')}
            </button>
          </li>))}</ul>}
      <button ref={addRef} type="button" className={button} disabled={busy || jobActive} {...lockProps} onClick={() => void add()}>{t('sfAdd')}</button>
      {jobActive && <p className="mt-1 text-small text-[var(--ink-2)]">{t('sfLockedNote')}</p>}
      <label className="flex items-start gap-2 mt-3 text-small cursor-pointer">
        <input type="checkbox" className="mt-0.5 shrink-0" checked={checkOnStart} disabled={busy}
          onChange={e => void changeCheckOnStart(e.target.checked)} />
        <span>{t('sfCheckOnStart')}<span className="block text-[var(--ink-2)]">{t('sfCheckOnStartHint')}</span></span>
      </label>
      <div className="flex items-center gap-2 mt-3 text-small">
        <span className="flex-1">{t('sfRejected').replace('{count}', String(rejected))}</span>
        <button type="button" className={`${button} shrink-0`} disabled={busy || jobActive || rejected === 0} {...lockProps} onClick={() => void reset()}>{t('sfResetRejected')}</button>
      </div>
      <p className="mt-2 rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{t('sfRules')}</p>
      {error && <p role="alert" className="mt-2 text-small text-[var(--crit)] whitespace-pre-wrap [overflow-wrap:anywhere]"><ErrorText error={error} /></p>}
    </section>
  );
}
