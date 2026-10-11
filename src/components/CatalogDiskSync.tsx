import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useT, useLanguage, useFormatCount } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { useModalDialog } from '../hooks/useModalDialog';
import { applyCatalogDiskSync, scanCatalogDisk, diskSyncError, type DiskSyncError, type DiskScan, type DiskApplied, type DiskEntry, type DiskFailedPath } from '../lib/api/diskSync';
import { Icon } from './Icon';

const button = 'px-3 py-2 rounded border border-[var(--line)] bg-[var(--panel)] text-body cursor-pointer disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

function SyncDialog({ scan, error, failedPaths, busy, onClose, onRetry, onApply }: {
  scan: DiskScan | null; error: DiskSyncError | null; failedPaths: DiskFailedPath[]; busy: boolean;
  onClose: () => void; onRetry: () => void; onApply: (remove: boolean) => void;
}) {
  const t = useT();
  const count = useFormatCount();
  const [remove, setRemove] = useState(false);
  const canApply = !!scan && (scan.newFiles.length > 0 || scan.newFolders.length > 0 || (remove && scan.missingFiles.length > 0));
  const ref = useModalDialog({ open: true, onClose, busy });
  const group = (entries: DiskEntry[]) => <ul className="space-y-1">{entries.map(e => <li key={e.path} title={e.path} className="rounded bg-[var(--panel-2)] p-2 break-all font-code text-small">{e.relativePath}<span className="block text-[var(--ink-3)]">{e.root}</span></li>)}</ul>;
  return createPortal(<div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4">
    <div ref={ref} role="dialog" aria-modal="true" aria-label={t('diskSyncTitle')} tabIndex={-1} className="w-[560px] max-w-full max-h-[85vh] flex flex-col rounded-lg bg-[var(--panel)] border border-[var(--line)] shadow-[var(--shadow)]">
      <h2 className="text-title font-semibold px-4 py-3 border-b border-[var(--line)]">{t('diskSyncTitle')}</h2>
      <div className="overflow-auto p-4 space-y-4 text-body">
        {failedPaths.length > 0 && <section><h3>{count(t('diskSyncUnreadable'), failedPaths.length)}</h3><ul>{failedPaths.map(({ path }) => <li key={path} className="font-code break-all">{path}</li>)}</ul></section>}
        {error ? <div role="alert" className="text-[var(--crit)] whitespace-pre-wrap break-words">{error.paths.length > 0 && t('diskSyncError')}<div>{t(error.code)}</div><ul>{error.paths.map(path => <li key={path} className="font-code break-all">{path}</li>)}</ul></div> : scan && <>
          <div className="flex flex-wrap gap-2">{([
            count(t('diskSyncNewFiles'), scan.newFiles.length), count(t('diskSyncNewFolders'), scan.newFolders.length), count(t('diskSyncMissing'), scan.missingFiles.length),
          ]).map(text => <span key={text} className="rounded-full px-3 py-1 bg-[var(--panel-2)] text-small">{text}</span>)}</div>
          {scan.skippedPaths.length > 0 && <section><h3 className="ui-label mb-2">{count(t('diskSyncSkipped'), scan.skippedPaths.length)}</h3><ul>{scan.skippedPaths.map(path => <li key={path} className="font-code break-all">{path}</li>)}</ul></section>}
          {(scan.newFiles.length > 0 || scan.newFolders.length > 0) && <section><h3 className="ui-label mb-2">{t('diskSyncNewHeading')}</h3>{group([...scan.newFolders, ...scan.newFiles])}</section>}
          {scan.missingFiles.length > 0 && <section><h3 className="ui-label mb-2">{t('diskSyncMissingHeading')}</h3>{group(scan.missingFiles)}
            <label className="flex gap-2 mt-4"><input type="checkbox" checked={remove} disabled={busy} onChange={e => setRemove(e.target.checked)} />{t('diskSyncRemove')}</label>
            <p className="text-small text-[var(--ink-2)] mt-2">{t('diskSyncRemoveHint')}</p>
          </section>}
        </>}
      </div>
      <div className="flex justify-end gap-2 p-4 border-t border-[var(--line)]">
        <button className={button} disabled={busy} onClick={onClose}>{t(canApply ? 'diskSyncLater' : 'diskSyncClose')}</button>
        {(error || canApply) && <button className={`${button} !bg-[var(--accent)] !text-[var(--accent-ink)]`} disabled={busy} onClick={() => error ? onRetry() : onApply(remove)}>{t(busy ? 'diskSyncRunning' : error ? 'diskSyncRetry' : 'diskSyncApply')}</button>}
      </div>
    </div>
  </div>, document.body);
}

export function CatalogDiskSync({ onApplied, active = true }: { onApplied?: () => Promise<unknown>; active?: boolean }) {
  const t = useT(); const count = useFormatCount(); const { language } = useLanguage();
  const { jobActive } = useImportLock();
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const [scan, setScan] = useState<DiskScan | null>(null);
  const [error, setError] = useState<DiskSyncError | null>(null);
  const [failedPaths, setFailedPaths] = useState<DiskFailedPath[]>([]);
  const [lastScan, setLastScan] = useState<Date | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [fresh, setFresh] = useState<DiskApplied | null>(null);
  useEffect(() => { if (toast) { const timer = setTimeout(() => setToast(null), 5000); return () => clearTimeout(timer); } }, [toast]);
  useEffect(() => { if (fresh) { const timer = setTimeout(() => setFresh(null), 6000); return () => clearTimeout(timer); } }, [fresh]);
  const run = useCallback(async () => {
    if (running.current || jobActive) return;
    running.current = true; setBusy(true); setError(null); setScan(null); setToast(null); setFailedPaths([]);
    try {
      const result = await scanCatalogDisk();
      if (result.unreachableRoots.length) setError({ code: 'diskSyncRootUnreachable', paths: result.unreachableRoots });
      else {
        setLastScan(new Date());
        if (result.newFiles.length || result.newFolders.length || result.missingFiles.length || result.skippedPaths.length) setScan(result);
        else setToast(t('diskSyncCurrent'));
      }
    } catch (e) { setError(diskSyncError(e)); }
    finally { running.current = false; setBusy(false); }
  }, [t, jobActive]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (!active || event.key !== 'F5' || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
      event.preventDefault();
      if (!document.querySelector('[role="dialog"], [aria-modal="true"]')) void run();
    };
    document.addEventListener('keydown', key);
    return () => document.removeEventListener('keydown', key);
  }, [run, active]);
  const apply = async (remove: boolean) => {
    if (!scan || running.current) return;
    running.current = true; setBusy(true);
    try {
      const result = await applyCatalogDiskSync(scan, remove);
      setScan(null);
      try { await onApplied?.(); }
      catch (e) { console.error('Catalog refresh after disk sync failed', e); }
      setFresh(result);
      const failures = result.failedPaths;
      if (failures.length && !result.fileIds.length && !result.folderIds.length && !result.removedCount) {
        setFailedPaths(failures);
      } else {
        const summary = [count(t('diskSyncNewFiles'), result.fileIds.length), count(t('diskSyncNewFolders'), result.folderIds.length), count(t('diskSyncRemoved'), result.removedCount)].join(' · ');
        setToast(failures.length ? `${summary}\n${count(t('diskSyncUnreadable'), failures.length)}\n${failures.map(f => f.path).join('\n')}` : summary);
      }
    } catch (e) { setScan(null); setError(diskSyncError(e)); }
    finally { running.current = false; setBusy(false); }
  };
  const selectors = fresh ? [...fresh.fileIds.map(id => `[data-model-id="${CSS.escape(id)}"]`), ...fresh.folderIds.map(id => `[data-sync-folder-id="${CSS.escape(id)}"]`)] : [];
  return <>
    {active && <button type="button" className={`${button} shrink-0 flex gap-2 items-center !py-1.5`} disabled={busy || jobActive} onClick={() => void run()} title={`${t('diskSyncLast')}: ${lastScan ? lastScan.toLocaleString(language) : t('diskSyncNever')} (F5)`}>
      <Icon name="restore" size={16} className={busy ? 'motion-safe:animate-spin' : undefined} />{t(busy ? 'diskSyncRunning' : 'diskSyncRefresh')}
    </button>}
    {(scan || error || failedPaths.length > 0) && <SyncDialog scan={scan} error={error} failedPaths={failedPaths} busy={busy} onClose={() => { setScan(null); setError(null); setFailedPaths([]); }} onRetry={() => void run()} onApply={remove => void apply(remove)} />}
    {toast && createPortal(<div role="status" className="whitespace-pre-wrap break-all max-w-[90vw] max-h-[50vh] overflow-auto fixed bottom-6 left-1/2 -translate-x-1/2 z-50 rounded-lg bg-[var(--ink)] text-[var(--bg)] px-4 py-3 shadow-lg">{toast}</div>, document.body)}
    {selectors.length > 0 && <style>{`${selectors.join(',')} { outline: 2px solid var(--accent); outline-offset: -2px; background-color: var(--accent-soft); }`}</style>}
  </>;
}
