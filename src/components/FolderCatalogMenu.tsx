import { canManageFiles, useCanManageFiles } from '../lib/webSession';
import { fileOp } from '../lib/fileOps';
import { fileOpsError } from '../lib/fileOpsErrors';
import { folderOpsMessages } from '../lib/folderOpsMessages';
import { isFolderSelfOrDescendant } from '../lib/folderTree';
import * as foldersApi from '../lib/api/folders';
import type { Folder } from '../types';
import { isReadOnly } from '../lib/readOnly';
import { displayName } from '../lib/displayName';
import { useImportLock } from '../hooks/ImportLockContext';
import { useEffect, useRef, useState, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '../lib/transport';
import { useT, useFormatCount, useLanguage } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';

type MenuProps = { folderId: string; name: string; x: number; y: number; onClose: () => void; onRemoved: () => void; returnFocus: RefObject<HTMLElement | null>; folders?: Folder[] };
export function FolderCatalogMenu(props: MenuProps) {
  return isReadOnly() ? <WebFolderCatalogMenu {...props} /> : <DesktopFolderCatalogMenu {...props} />;
}
interface Summary { name: string; subfolderCount: number; modelCount: number }
function DesktopFolderCatalogMenu({ folderId, name, x, y, onClose, onRemoved, returnFocus }: {
  folderId: string; name: string; x: number; y: number;
  onClose: () => void; onRemoved: () => void; returnFocus: RefObject<HTMLElement | null>;
}) {
  const { lockProps } = useImportLock();
  const t = useT();
  const formatCount = useFormatCount();
  const [dialog, setDialog] = useState(false);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose); closeRef.current = onClose;
  useEffect(() => {
    if (dialog) return;
    menu.current?.querySelector('button')?.focus();
    const outside = (e: MouseEvent) => { if (!menu.current?.contains(e.target as Node)) closeRef.current(); };
    const escape = (e: KeyboardEvent) => {
      if (e.key === 'Escape' || e.key === 'Tab') { e.preventDefault(); closeRef.current(); returnFocus.current?.focus(); }
    };
    document.addEventListener('mousedown', outside);
    document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('mousedown', outside); document.removeEventListener('keydown', escape); };
  }, [dialog, returnFocus]);
  useEffect(() => {
    if (!dialog) return;
    let cancelled = false;
    invoke<Summary>('folder_removal_summary', { folderId }).then((value) => {
      if (!cancelled) setSummary(value);
    }).catch((e) => { if (!cancelled) setError(toAppError(e)); });
    return () => { cancelled = true; };
  }, [dialog, folderId]);
  if (isReadOnly()) return null;
  if (!dialog) return createPortal(
    <div ref={menu} role="menu" className="fixed z-50 min-w-[200px] overflow-hidden rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-lg"
      style={{ left: Math.max(0, Math.min(x, window.innerWidth - 260)), top: Math.max(0, Math.min(y, window.innerHeight - 50)) }}>
      <button {...lockProps} role="menuitem" className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)] focus-visible:bg-[var(--panel-2)] outline-none" onClick={() => setDialog(true)}>{t('removeCatalog')}</button>
    </div>, document.body,
  );
  return <CatalogActionDialog title={t('removeFolderQuestion').replace('{name}', displayName(summary?.name ?? name))} onClose={() => { if (!busy) onClose(); }} returnFocus={returnFocus}>
    {summary && <ul className="list-disc pl-5 space-y-1">
      <li>{formatCount(t('removeFolderList'), summary.subfolderCount).replace('{name}', displayName(summary.name))}</li>
      <li>{formatCount(t('removeFolderModels'), summary.modelCount)}</li>
    </ul>}
    <p className="text-[var(--good)] font-semibold">{t('removeFolderSafe')}</p>
    <p>{t('removeFolderReimport')}</p>
    {error && <div role="alert"><ErrorText error={error} /></div>}
    <div className="flex gap-2 justify-end">
      <button data-initial-focus className={catalogActionButton} disabled={busy} onClick={onClose}>{t('cancel')}</button>
      <button className={`${catalogActionBase} hover:bg-[var(--panel-2)] hover:text-[var(--ink)] border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)]`} disabled={busy || !summary} {...lockProps} onClick={async () => {
        setBusy(true); setError(null);
        try { await invoke('remove_folder_from_catalog', { folderId }); onRemoved(); onClose(); }
        catch (e) {
          const parsed = toAppError(e);
          if (!parsed.unexpected && parsed.message.startsWith('Das ist der Speicherort des Katalogs.')) parsed.message = t('catalogBaseProtected');
          setError(parsed); setBusy(false);
        }
      }}>{t('removeCatalog')}</button>
    </div>
  </CatalogActionDialog>;
}

function WebFolderCatalogMenu({ folderId, name, x, y, onClose, onRemoved, returnFocus, folders = [] }: MenuProps) {
  const canManage = useCanManageFiles();
  const { jobActive } = useImportLock();
  const { language } = useLanguage();
  const labels = folderOpsMessages(language);
  const t = useT();
  const formatCount = useFormatCount();
  const [action, setAction] = useState<'create' | 'rename' | 'move' | 'remove' | null>(null);
  const [draft, setDraft] = useState(name);
  const [target, setTarget] = useState('');
  const [summary, setSummary] = useState<Summary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (action || !canManage) return;
    menu.current?.querySelector('button')?.focus();
    const outside = (e: MouseEvent) => { if (!menu.current?.contains(e.target as Node)) onClose(); };
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape' || e.key === 'Tab') { onClose(); returnFocus.current?.focus(); } };
    document.addEventListener('mousedown', outside); document.addEventListener('keydown', key);
    return () => { document.removeEventListener('mousedown', outside); document.removeEventListener('keydown', key); };
  }, [action, canManage, onClose, returnFocus]);
  useEffect(() => {
    if (action !== 'remove' || !canManageFiles()) return;
    let cancelled = false;
    invoke<Summary>('folder_removal_summary', { folderId }).then(value => {
      if (!cancelled) setSummary(value);
    }).catch(reason => { if (!cancelled) setError(fileOpsError(reason).message); });
    return () => { cancelled = true; };
  }, [action, folderId]);
  if (!canManage) return null;
  const titles = { create: labels.create, rename: labels.rename, move: labels.move, remove: t('removeCatalog') };
  if (!action) return createPortal(<div ref={menu} role="menu" className="fixed z-50 rounded border bg-[var(--panel)] p-2 shadow-lg" style={{ left: Math.max(0, Math.min(x, window.innerWidth - 260)), top: Math.max(0, Math.min(y, window.innerHeight - 200)) }}>
    {(['create', 'rename', 'move', 'remove'] as const).map(item => <button key={item} role="menuitem" disabled={jobActive} className="block w-full p-2 text-left hover:bg-[var(--panel-2)]" onClick={() => { setDraft(item === 'create' ? '' : name); setAction(item); }}>{titles[item]}</button>)}
  </div>, document.body);
  const parentId = folders.find(folder => folder.id === folderId)?.parentId;
  const targets = folders.filter(folder => folder.id !== parentId && !isFolderSelfOrDescendant(folder.id, folderId, folders));
  const submit = async () => {
    if (busy || jobActive || !canManageFiles()) return;
    if (action === 'remove' && !summary) return;
    if (action === 'move' && !targets.some(folder => folder.id === target)) return;
    setBusy(true); setError(null);
    try {
      if (action === 'create') await foldersApi.createFolder(folderId, draft);
      if (action === 'rename') await foldersApi.renameFolder(folderId, draft);
      if (action === 'move') await foldersApi.moveFolder(folderId, target);
      if (action === 'remove') { await fileOp('remove_folder_from_catalog', { folderId }); onRemoved(); }
      onClose();
    } catch (reason) { setError(toAppError(reason).message); }
    finally { setBusy(false); }
  };
  return <CatalogActionDialog title={titles[action]} returnFocus={returnFocus} onClose={() => { if (!busy) onClose(); }}>
    <p>{displayName(name)}</p>
    {(action === 'create' || action === 'rename') && <label>{labels.name}<input data-initial-focus value={draft} disabled={busy} onChange={event => setDraft(event.target.value)} className="block w-full border p-2 bg-[var(--panel)]" /></label>}
    {action === 'move' && <label>{labels.target}<select data-initial-focus value={target} disabled={busy} onChange={event => setTarget(event.target.value)} className="block w-full border p-2 bg-[var(--panel)]"><option value="">—</option>{targets.map(folder => <option key={folder.id} value={folder.id}>{displayName(folder.name)}</option>)}</select></label>}
    {(action === 'move' || action === 'rename') && <p>{labels.contents}</p>}
    {action === 'remove' && <>
      {summary ? <ul><li>{formatCount(t('removeFolderList'), summary.subfolderCount).replace('{name}', displayName(summary.name))}</li><li>{formatCount(t('removeFolderModels'), summary.modelCount)}</li></ul> : <p>{labels.loading}</p>}
      <p>{t('removeFolderSafe')}</p><p>{t('removeFolderReimport')}</p>
    </>}
    {error && <p role="alert">{error}</p>}
    <div className="flex justify-end gap-2"><button className={catalogActionButton} disabled={busy} onClick={onClose}>{t('cancel')}</button><button className={catalogActionButton} disabled={busy || jobActive || (action === 'remove' && !summary) || (action === 'move' && !target) || ((action === 'create' || action === 'rename') && !draft.trim())} onClick={() => void submit()}>{labels.confirm}</button></div>
  </CatalogActionDialog>;
}
