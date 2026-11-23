import { Select } from './Select';
import { useEffect, useRef, useState, type RefObject, type ReactNode } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { canManageFiles, useCanManageFiles } from '../lib/webSession';
import { emptyWebTrash, fileOp, fileOpsBatch, type FileLabels, type TrashSummary } from '../lib/fileOps';
import { invoke } from '../lib/transport';
import { messageOf } from '../lib/errors';
import { webFileNameParts } from '../lib/webFileName';
import type { Folder } from '../types';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { useImportLock } from '../hooks/ImportLockContext';

type Action = 'rename' | 'move' | 'trash' | 'restore' | 'permanent' | 'empty';
export { webFileNameParts };

/** Web-only actions share one confirmation and never route through desktop handlers. */
export function WebFileActions({ hideTrigger = false, requestMoveConfirm = false, returnFocus, menuAction, menuLabel, menuClassName, menuShortcut, fileIds = [], names, hiddenCount = 0, name, trash = false, desktopTrash = false, empty = false, onDone, requestTrashConfirm = false, onConfirmationClosed, onTrashRequest }: {
  hideTrigger?: boolean; requestMoveConfirm?: boolean; returnFocus?: RefObject<HTMLButtonElement | null>;
  menuAction?: 'rename' | 'move' | 'trash'; menuLabel?: ReactNode; menuClassName?: string; menuShortcut?: string;
  fileIds?: string[]; names?: string[]; hiddenCount?: number; name?: string; trash?: boolean; desktopTrash?: boolean; empty?: boolean; onDone?: () => void; requestTrashConfirm?: boolean; onConfirmationClosed?: () => void;
  /** The caller decides how a trash request goes on (it may ask something first); it answers through `requestTrashConfirm`. */
  onTrashRequest?: () => void;
}) {
  const allowed = useCanManageFiles();
  const { lockProps } = useImportLock();
  const { language } = useLanguage();
  const t = useT();
  const c = {
    looseCompanions: t('webFileLooseCompanions'),
    move: t('webFileMove'),
    trashLabel: t('webFileTrashLabel'),
    target: t('webFileTarget'),
    rename: t('webFileRename'),
    trash: t('webFileTrash'),
    restore: t('webFileRestore'),
    permanent: t('webFilePermanent'),
    empty: t('webFileEmpty'),
    password: t('webFilePassword'),
    summary: t('webFileSummary'),
    unavailable: t('webFileUnavailable'),
    kept: t('webFileKept'),
    progress: t('webFileProgress'),
    loading: t('webFileLoading'),
    files: t('webFileFiles'),
    file1: t('webFileFile1'),
    hidden: t('webFileHidden'),
    more: t('webFileMore'),
    cancelling: t('webFileCancelling'),
    companions: t('webFileCompanions'),
    desktopEntries: t('webFileDesktopEntries'),
    moveBlocked: t('webFileMoveBlocked'),
    moveBlockedHint: t('webFileMoveBlockedHint'),
    desktopEntry: t('webFileDesktopEntry'),
  };
  const [action, setAction] = useState<Action | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [base, setBase] = useState('');
  const [password, setPassword] = useState('');
  const [target, setTarget] = useState('');
  const [folders, setFolders] = useState<Folder[]>([]);
  const [summary, setSummary] = useState<TrashSummary | null>(null);
  const [progress, setProgress] = useState<[number, number] | null>(null);
  const opener = useRef<HTMLButtonElement | null>(null);
  const passwordField = useRef<HTMLInputElement | null>(null);
  const submitting = useRef(false);
  const ids = useRef<string[]>([]);
  const snapshotNames = useRef<{ names: string[]; hidden: number }>({ names: [], hidden: 0 });
  const cancelRequested = useRef(false);
  const [cancelling, setCancelling] = useState(false);
  const previousRequest = useRef(false);
  useEffect(() => {
    if (requestTrashConfirm && !previousRequest.current && allowed) {
      ids.current = [...fileIds]; snapshotNames.current = { names: [...(names ?? [])], hidden: hiddenCount }; setAction('trash'); setError(null); setNotice(null);
    }
    previousRequest.current = requestTrashConfirm;
  }, [requestTrashConfirm, allowed, fileIds, names, hiddenCount]);
  const previousMoveRequest = useRef(false);
  useEffect(() => {
    if (requestMoveConfirm && !previousMoveRequest.current && allowed) {
      opener.current = returnFocus?.current ?? null;
      ids.current = [...fileIds]; snapshotNames.current = { names: [...(names ?? [])], hidden: hiddenCount };
      setAction('move'); setError(null); setNotice(null); setTarget(''); setSummary(null); setProgress(null);
    }
    previousMoveRequest.current = requestMoveConfirm;
  }, [requestMoveConfirm, allowed, fileIds, names, hiddenCount, returnFocus]);
  useEffect(() => { if (!allowed) setPassword(''); }, [allowed]);
  // After a refused confirmation the emptied field is the next thing to type into; it is disabled while busy, so focus waits for that to end.
  useEffect(() => { if (error && !busy && !password) passwordField.current?.focus(); }, [error, busy, password]);
  useEffect(() => {
    if (!action || (action !== 'move' && action !== 'empty')) return;
    let current = true;
    setLoading(true);
    const load = action === 'move'
      ? invoke<Folder[]>('list_folders').then(value => { if (current) setFolders(value); })
      : invoke<TrashSummary>('empty_trash_summary').then(value => { if (current) setSummary(value); });
    void load.catch(reason => { if (current) setError(messageOf(reason)); }).finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [action]);
  if (import.meta.env.VITE_TARGET !== 'web' || (!allowed && !action)) return null;
  const labels = { rename: t('renameLabel'), move: c.move, trash: c.trashLabel, restore: t('restoreLabel'), permanent: t('deletePermanentlyLabel'), empty: t('emptyTrashButton') };
  // While a run is active, closing means "stop before the next file/block"; the request already sent is never aborted.
  const close = () => {
    if (submitting.current) { cancelRequested.current = true; setCancelling(true); return; }
    setAction(null); setPassword(''); onConfirmationClosed?.();
  };
  const open = (value: Action, button: HTMLButtonElement) => {
    opener.current = button;
    if (value === 'trash' && onTrashRequest) { onTrashRequest(); return; }
    ids.current = [...fileIds]; snapshotNames.current = { names: [...(names ?? [])], hidden: hiddenCount };
    setAction(value); setError(null); setNotice(null); setPassword(''); setTarget('');
    setSummary(null); setProgress(null); setBase(webFileNameParts(name ?? '').base);
  };
  // Names for the refusal text; the selection's names run parallel to its IDs, anything else stays unnamed.
  const labelsOf = (): FileLabels => {
    const list = ids.current; const names = snapshotNames.current.names;
    if (list.length === 1 && name) return { [list[0]]: name };
    return names.length === list.length ? Object.fromEntries(list.map((id, i) => [id, names[i]]).filter(([id, label]) => label !== id)) : {};
  };
  const submit = async () => {
    if (!action || submitting.current || !canManageFiles() || lockProps.disabled) return;
    submitting.current = true; cancelRequested.current = false; setCancelling(false); setBusy(true); setError(null);
    const secret = password; setPassword('');
    try {
      if (action === 'rename') await fileOp('rename_file', { fileId: ids.current[0], name: base }, { labels: labelsOf() });
      if (action === 'move') await fileOpsBatch('move_files_to_folder', ids.current, { folderId: target }, () => cancelRequested.current, labelsOf());
      if (action === 'trash') await fileOpsBatch('delete_files', ids.current, {}, () => cancelRequested.current, labelsOf());
      if (action === 'restore') await fileOp('restore_file', { fileId: ids.current[0] }, { labels: labelsOf() });
      if (action === 'permanent') {
        const result = await fileOp<{ kept_on_disk: boolean }>('delete_file_permanently', { fileId: ids.current[0], confirm: true, password: secret }, { quietKept: true });
        if (result.kept_on_disk) { setNotice(c.kept); return; }
      }
      if (action === 'empty' && summary) await emptyWebTrash(summary, secret, (done, total) => setProgress([done, total]), () => cancelRequested.current);
      setAction(null); onConfirmationClosed?.(); onDone?.();
    } catch (reason) {
      setError(messageOf(reason));
      // A changed snapshot needs a fresh count and a new, explicit password confirmation.
      if (action === 'empty' && canManageFiles()) {
        setSummary(null); setLoading(true);
        try { setSummary(await invoke<TrashSummary>('empty_trash_summary')); }
        catch (summaryError) { setError(messageOf(summaryError)); }
        finally { setLoading(false); }
      }
    }
    finally { submitting.current = false; cancelRequested.current = false; setCancelling(false); setBusy(false); }
  };
  const actions: Action[] = menuAction ? [menuAction] : empty ? ['empty'] : trash ? (desktopTrash ? ['permanent'] : ['restore', 'permanent']) : [...(fileIds.length === 1 && name ? ['rename' as const] : []), 'move', 'trash'];
  // `contents`: the buttons join the caller's (wrapping) action row; the dialog is portalled anyway.
  return <div className="contents">
    {allowed && !hideTrigger && actions.map(value => <button key={value} type="button" className={menuClassName ?? `${catalogActionButton} whitespace-nowrap`} aria-keyshortcuts={menuShortcut} disabled={busy || lockProps.disabled || (!empty && !fileIds.length)} aria-haspopup="dialog" onClick={event => open(value, event.currentTarget)}>{menuLabel ?? labels[value]}</button>)}
    {action && <CatalogActionDialog title={labels[action]} onClose={close} returnFocus={opener}>
      <div aria-busy={busy || loading} className="space-y-3">
        {['trash', 'restore', 'permanent', 'empty'].includes(action) && <p>{c[action as 'trash' | 'restore' | 'permanent' | 'empty']}</p>}
        {!empty && !snapshotNames.current.names.length && <p>{name ? `${name} (${ids.current.length})` : `${ids.current.length}`} </p>}
        {!empty && snapshotNames.current.names.length > 0 && <div>
          <p>{(ids.current.length === 1 ? c.file1 : c.files.replace('{n}', String(ids.current.length))) + (snapshotNames.current.hidden > 0 ? c.hidden.replace('{m}', String(snapshotNames.current.hidden)) : '')}</p>
          <ul className="list-disc pl-5">{snapshotNames.current.names.slice(0, 5).map((fileName, index) => <li key={index}>{fileName}</li>)}</ul>
          {ids.current.length > 5 && <p>{c.more.replace('{n}', String(ids.current.length - 5))}</p>}
        </div>}
        {action === 'rename' && <label className="block">{c.rename}<input className="block w-full border p-2" value={base} disabled={busy} onChange={e => setBase(e.target.value)} /><span>{webFileNameParts(name ?? '').extension}</span></label>}
        {action === 'move' && <label className="block">{c.target}<Select className="block w-full border p-2" value={target} disabled={busy || loading} onValueChange={value => setTarget(value)}><option value="">—</option>{folders.map(folder => folder.moveTarget === false
          ? <option key={folder.id} value={folder.id} disabled>{folder.name} (#{folder.id}) — {c.moveBlocked}</option>
          : <option key={folder.id} value={folder.id}>{folder.name} (#{folder.id})</option>)}</Select></label>}
        {action === 'move' && folders.some(folder => folder.moveTarget === false) && <p>{c.moveBlockedHint}</p>}
        {summary && <p>{c.summary}: {summary.count.toLocaleString(language)} / {(summary.bytes + (summary.looseCompanionBytes ?? 0)).toLocaleString(language)}</p>}
        {summary && (summary.companionCount ?? 0) > 0 && <p>{c.companions.replace('{n}', (summary.companionCount ?? 0).toLocaleString(language))}</p>}
        {summary && (summary.looseCompanionCount ?? 0) > 0 && <p>{c.looseCompanions.replace('{n}', (summary.looseCompanionCount ?? 0).toLocaleString(language))}</p>}
        {summary && (summary.blockedByCompanions ?? 0) > 0 && <p>{summary.blockedByCompanions === 1 ? c.desktopEntry : c.desktopEntries.replace('{n}', (summary.blockedByCompanions ?? 0).toLocaleString(language))}</p>}
        {(action === 'permanent' || action === 'empty') && !notice && <label className="block">{c.password}<input ref={passwordField} className="block w-full border p-2" type="password" autoComplete="new-password" data-1p-ignore data-lpignore="true" data-form-type="other" value={password} disabled={busy || !allowed} onChange={e => setPassword(e.target.value)} /></label>}
        {loading && <p role="status">{c.loading}</p>}
        {cancelling && <p role="status">{c.cancelling}</p>}
        {progress && <p role="status">{c.progress}: {progress[0]} / {progress[1]}</p>}
        {!allowed && <p role="alert">{c.unavailable}</p>}
        {error && <p role="alert">{error}</p>}
        {notice && <p role="status">{notice}</p>}
        <div className="flex justify-end gap-2">
          <button data-initial-focus type="button" className={catalogActionButton} disabled={cancelling} onClick={close}>{t('cancel')}</button>
          {!notice && <button type="button" className={catalogActionButton} disabled={busy || loading || !allowed || lockProps.disabled || (action === 'rename' && !base.trim()) || (action === 'move' && !target) || ((action === 'permanent' || action === 'empty') && !password) || (action === 'empty' && (!summary || summary.count + (summary.looseCompanionCount ?? 0) === 0))} onClick={() => void submit()}>{labels[action]}</button>}
        </div>
      </div>
    </CatalogActionDialog>}
  </div>;
}
