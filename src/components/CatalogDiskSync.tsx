import { displayName } from '../lib/displayName';
import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useT, useLanguage, useFormatCount } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { useModalDialog } from '../hooks/useModalDialog';
import { applyCatalogDiskSync, scanCatalogDisk, diskSyncError, type DiskSyncError, type DiskScan, type DiskApplied, type DiskEntry, type DiskFailedPath, type DiskPicked, type DiskRename } from '../lib/api/diskSync';
import { Icon } from './Icon';

const button = 'px-3 py-2 rounded border border-[var(--line)] bg-[var(--panel)] text-body cursor-pointer disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

const folderOf = (relativePath: string) => relativePath.slice(0, Math.max(relativePath.lastIndexOf('/'), relativePath.lastIndexOf('\\'), 0));
const withToggled = (set: ReadonlySet<string>, value: string, on: boolean) => {
  const next = new Set(set);
  if (on) next.add(value); else next.delete(value);
  return next;
};
// New files and renames start ticked; copies of existing models are opt-in.
const initialPaths = (scan: DiskScan | null) => new Set(scan?.newFiles.map(e => e.path));
const initialRenames = (scan: DiskScan | null) => new Set(scan?.renamedFiles.map(r => r.id));
const tag = 'rounded-full px-2 py-0.5 bg-[var(--accent-soft)] text-[var(--accent)] text-small font-semibold normal-case tracking-normal';
const hintBox = 'rounded bg-[var(--panel-2)] px-2.5 py-1.5 mt-2 text-small text-[var(--ink-2)]';

function SyncDialog({ scan, error, failedPaths, busy, onClose, onRetry, onApply }: {
  scan: DiskScan | null; error: DiskSyncError | null; failedPaths: DiskFailedPath[]; busy: boolean;
  onClose: () => void; onRetry: () => void; onApply: (remove: boolean, picked: DiskPicked) => void;
}) {
  const t = useT();
  const count = useFormatCount();
  const [remove, setRemove] = useState(false);
  const [paths, setPaths] = useState<ReadonlySet<string>>(() => initialPaths(scan));
  const [renameIds, setRenameIds] = useState<ReadonlySet<string>>(() => initialRenames(scan));
  useEffect(() => { setPaths(initialPaths(scan)); setRenameIds(initialRenames(scan)); }, [scan]);
  const selectable = !!scan && (scan.newFiles.length > 0 || scan.renamedFiles.length > 0 || scan.duplicateFiles.length > 0);
  const hasMissing = !!scan && scan.missingFiles.length > 0;
  // The reset effect runs after the render that drops the scan; do not trust stale ticks until then.
  const canApply = (!!scan && paths.size + renameIds.size > 0) || (remove && hasMissing);
  const ref = useModalDialog({ open: true, onClose, busy });
  const rows = (e: DiskEntry) => <>
    <div className="flex gap-2">
      <span className="w-[100px] shrink-0 text-[var(--ink-3)]">{t('diskSyncFileLabel')}</span>
      <span className="min-w-0 break-all font-code">{displayName(e.relativePath)}</span>
    </div>
    <div className="my-1.5 border-t border-[var(--line)]" />
    <div className="flex gap-2">
      <span className="w-[100px] shrink-0 text-[var(--ink-3)]">{t('diskSyncRootLabel')}</span>
      <span className="min-w-0 break-all font-code">{displayName(e.root)}</span>
    </div>
  </>;
  const group = (entries: DiskEntry[]) => <ul className="space-y-1">{entries.map(e => (
    <li key={e.path} title={displayName(e.path)} className="rounded bg-[var(--panel-2)] p-2 text-small">{rows(e)}</li>
  ))}</ul>;
  // A real checkbox per file; the label spans the entry so the whole row is a click target.
  const pickable = <T extends DiskEntry,>(entries: T[], note?: (e: T) => string) => <ul className="space-y-1">{entries.map(e => (
    <li key={e.path} title={displayName(e.path)} className="rounded bg-[var(--panel-2)] p-2 text-small">
      <label className="flex gap-2 items-start cursor-pointer">
        <input type="checkbox" className="mt-0.5 shrink-0" aria-label={displayName(e.relativePath)} checked={paths.has(e.path)} disabled={busy} onChange={ev => setPaths(withToggled(paths, e.path, ev.target.checked))} />
        <span className="min-w-0 flex-1">{rows(e)}{note && <span className="block mt-1.5 text-[var(--ink-2)]">{note(e)}</span>}</span>
      </label>
    </li>
  ))}</ul>;
  const renameNote = (r: DiskRename) => {
    const [from, to] = [folderOf(r.oldRelativePath), folderOf(r.newRelativePath)];
    const what = from === to ? t('diskSyncRenamedFileSub')
      : t('diskSyncRenamedFolderSub').replace('{from}', displayName(from || t('diskSyncTopLevel'))).replace('{to}', displayName(to || t('diskSyncTopLevel')));
    return `${what} · ${t('diskSyncKeepsData')}`;
  };
  const renamed = (entries: DiskRename[]) => <ul className="space-y-1">{entries.map(r => {
    const [oldName, newName] = [displayName(r.oldRelativePath), displayName(r.newRelativePath)];
    return <li key={r.id} title={`${displayName(r.oldPath)} → ${displayName(r.newPath)}`} className="rounded bg-[var(--panel-2)] p-2 text-small">
      <label className="flex gap-2 items-start cursor-pointer">
        <input type="checkbox" className="mt-0.5 shrink-0" aria-label={`${oldName} → ${newName}`} checked={renameIds.has(r.id)} disabled={busy} onChange={ev => setRenameIds(withToggled(renameIds, r.id, ev.target.checked))} />
        <span className="min-w-0 flex-1">
          <span className="break-all font-code"><s className="text-[var(--ink-3)]">{oldName}</s> <span aria-hidden="true" className="font-bold text-[var(--accent)]">→</span> {newName}</span>
          <span className="block mt-1 text-[var(--ink-2)]">{renameNote(r)}</span>
        </span>
      </label>
    </li>;
  })}</ul>;
  return createPortal(<div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4">
    <div ref={ref} role="dialog" aria-modal="true" aria-label={t('diskSyncTitle')} tabIndex={-1} className="w-[560px] max-w-full max-h-[85vh] flex flex-col rounded-lg bg-[var(--panel)] border border-[var(--line)] shadow-[var(--shadow)]">
      <h2 className="text-title font-semibold px-4 py-3 border-b border-[var(--line)]">{t('diskSyncTitle')}</h2>
      <div className="overflow-auto p-4 space-y-4 text-body">
        {failedPaths.length > 0 && <section><h3>{count(t('diskSyncUnreadable'), failedPaths.length)}</h3><ul>{failedPaths.map(({ path }) => <li key={path} className="font-code break-all">{displayName(path)}</li>)}</ul></section>}
        {error ? <div role="alert" className="text-[var(--crit)] whitespace-pre-wrap break-words">{error.paths.length > 0 && t('diskSyncError')}<div>{t(error.code)}</div><ul>{error.paths.map(path => <li key={path} className="font-code break-all">{displayName(path)}</li>)}</ul></div> : scan && <>
          <div className="flex flex-wrap gap-2">{([
            ...(scan.renamedFiles.length > 0 ? [count(t('diskSyncRenamed'), scan.renamedFiles.length)] : []),
            count(t('diskSyncNewFiles'), scan.newFiles.length), count(t('diskSyncNewFolders'), scan.newFolders.length), count(t('diskSyncMissing'), scan.missingFiles.length),
            ...(scan.duplicateFiles.length > 0 ? [count(t('diskSyncDuplicates'), scan.duplicateFiles.length)] : []),
          ]).map(text => <span key={text} className="rounded-full px-3 py-1 bg-[var(--panel-2)] text-small">{text}</span>)}</div>
          {scan.skippedPaths.length > 0 && <section><h3 className="ui-label mb-2">{count(t('diskSyncSkipped'), scan.skippedPaths.length)}</h3><ul>{scan.skippedPaths.map(path => <li key={path} className="font-code break-all">{displayName(path)}</li>)}</ul></section>}
          {scan.renamedFiles.length > 0 && <section><h3 className="ui-label mb-2 flex flex-wrap gap-2 items-center">{t('diskSyncRenamedHeading')}<span className={tag}>{t('diskSyncPreselected')}</span></h3>{renamed(scan.renamedFiles)}<p className={hintBox}>{t('diskSyncRenamedHint')}</p></section>}
          {(scan.newFiles.length > 0 || scan.newFolders.length > 0) && <section><h3 className="ui-label mb-2">{t('diskSyncNewHeading')}</h3>
            {scan.newFolders.length > 0 && <div className="mb-1">{group(scan.newFolders)}</div>}
            {pickable(scan.newFiles)}</section>}
          {scan.duplicateFiles.length > 0 && <section><h3 className="ui-label mb-2 flex flex-wrap gap-2 items-center">{t('diskSyncDuplicateHeading')}<span className={tag}>{t('diskSyncNotPreselected')}</span></h3>
            {pickable(scan.duplicateFiles, e => t('diskSyncDuplicateOf').replace('{name}', displayName(e.duplicateOfPath)))}
            <p className={hintBox}>{t('diskSyncDuplicateHint')}</p></section>}
          {scan.missingFiles.length > 0 && <section><h3 className="ui-label mb-2">{t('diskSyncMissingHeading')}</h3>{group(scan.missingFiles)}
            <label className="flex gap-2 mt-4"><input type="checkbox" checked={remove} disabled={busy} onChange={e => setRemove(e.target.checked)} />{t('diskSyncRemove')}</label>
            <p className="text-small text-[var(--ink-2)] mt-2">{t('diskSyncRemoveHint')}</p>
          </section>}
        </>}
      </div>
      <div className="flex justify-end gap-2 p-4 border-t border-[var(--line)]">
        <button className={button} disabled={busy} onClick={onClose}>{t(selectable || canApply ? 'diskSyncLater' : 'diskSyncClose')}</button>
        {(error || selectable || canApply) && <button className={`${button} !bg-[var(--accent)] !text-[var(--accent-ink)]`} disabled={busy || (!error && !canApply)} onClick={() => error ? onRetry() : onApply(remove, { paths, renameIds })}>{t(busy ? 'diskSyncRunning' : error ? 'diskSyncRetry' : selectable ? 'diskSyncApply' : 'diskSyncRemoveAction')}</button>}
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
        if (result.newFiles.length || result.newFolders.length || result.renamedFiles.length || result.duplicateFiles.length || result.missingFiles.length || result.skippedPaths.length) setScan(result);
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
  const apply = async (remove: boolean, picked: DiskPicked) => {
    if (!scan || running.current) return;
    running.current = true; setBusy(true);
    try {
      const result = await applyCatalogDiskSync(scan, remove, picked);
      setScan(null);
      try { await onApplied?.(); }
      catch (e) { console.error('Catalog refresh after disk sync failed', e); }
      setFresh(result);
      const failures = result.failedPaths;
      if (failures.length && !result.fileIds.length && !result.folderIds.length && !result.removedCount && !result.renamedCount) {
        setFailedPaths(failures);
      } else {
        const summary = [...(result.renamedCount > 0 ? [count(t('diskSyncRenamed'), result.renamedCount)] : []), count(t('diskSyncNewFiles'), result.fileIds.length), count(t('diskSyncNewFolders'), result.folderIds.length), count(t('diskSyncRemoved'), result.removedCount)].join(' · ');
        setToast(failures.length ? `${summary}\n${count(t('diskSyncUnreadable'), failures.length)}\n${failures.map(f => displayName(f.path)).join('\n')}` : summary);
      }
    } catch (e) { setScan(null); setError(diskSyncError(e)); }
    finally { running.current = false; setBusy(false); }
  };
  const selectors = fresh ? [...[...fresh.fileIds, ...(fresh.renamedIds ?? [])].map(id => `[data-model-id="${CSS.escape(id)}"]`), ...fresh.folderIds.map(id => `[data-sync-folder-id="${CSS.escape(id)}"]`)] : [];
  return <>
    {active && <button type="button" className={`${button} shrink-0 flex gap-2 items-center !py-1.5`} disabled={busy || jobActive} onClick={() => void run()} title={`${t('diskSyncLast')}: ${lastScan ? lastScan.toLocaleString(language) : t('diskSyncNever')} (F5)`}>
      <Icon name="restore" size={16} className={busy ? 'motion-safe:animate-spin' : undefined} />{t(busy ? 'diskSyncRunning' : 'diskSyncRefresh')}
    </button>}
    {(scan || error || failedPaths.length > 0) && <SyncDialog scan={scan} error={error} failedPaths={failedPaths} busy={busy} onClose={() => { setScan(null); setError(null); setFailedPaths([]); }} onRetry={() => void run()} onApply={(remove, picked) => void apply(remove, picked)} />}
    {toast && createPortal(<div role="status" className="whitespace-pre-wrap break-all max-w-[90vw] max-h-[50vh] overflow-auto fixed bottom-6 left-1/2 -translate-x-1/2 z-50 rounded-lg bg-[var(--ink)] text-[var(--bg)] px-4 py-3 shadow-lg">{toast}</div>, document.body)}
    {selectors.length > 0 && <style>{`${selectors.join(',')} { outline: 2px solid var(--accent); outline-offset: -2px; background-color: var(--accent-soft); }`}</style>}
  </>;
}
