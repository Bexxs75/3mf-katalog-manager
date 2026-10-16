import { useEffect, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { canManageFiles, useCanManageFiles } from '../lib/webSession';
import { emptyWebTrash, fileOp, fileOpsBatch, type TrashSummary } from '../lib/fileOps';
import { invoke } from '../lib/transport';
import { messageOf } from '../lib/errors';
import { webFileNameParts } from '../lib/webFileName';
import type { Folder } from '../types';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { useImportLock } from '../hooks/ImportLockContext';

const copy = {
  de: { move: 'Verschieben', trashLabel: 'In den Papierkorb', target: 'Zielordner', rename: 'Neuer Basisname', trash: 'Ausgewählte Dateien in den Papierkorb des Servers verschieben? Die Dateien werden an ihrem bisherigen Ort entfernt und können aus dem Papierkorb wiederhergestellt werden.', restore: 'Datei am ursprünglichen Ort wiederherstellen?', permanent: 'Datei unwiderruflich löschen?', empty: 'Papierkorb unwiderruflich leeren?', password: 'Passwort zur Bestätigung', summary: 'Dateien / Bytes', unavailable: 'Dateiverwaltung ist nicht mehr verfügbar.', kept: 'Der Katalogeintrag wurde entfernt. Die Datei bleibt auf dem Datenträger.', progress: 'Bearbeitet', loading: 'Wird geladen…', files: '{n} Dateien', file1: '1 Datei', hidden: ', davon {m} nicht sichtbar', more: '… und {n} weitere', cancelling: 'Wird abgebrochen…' },
  en: { move: 'Move', trashLabel: 'Move to trash', target: 'Destination folder', rename: 'New base name', trash: 'Move selected files to the server trash? Files are removed from their current location and can be restored from trash.', restore: 'Restore file to its original location?', permanent: 'Permanently delete file?', empty: 'Permanently empty trash?', password: 'Password to confirm', summary: 'Files / bytes', unavailable: 'File management is no longer available.', kept: 'The catalog entry was removed. The file remains on disk.', progress: 'Processed', loading: 'Loading…', files: '{n} files', file1: '1 file', hidden: ', {m} of them not visible', more: '… and {n} more', cancelling: 'Cancelling…' },
  es: { move: 'Mover', trashLabel: 'Mover a la papelera', target: 'Carpeta de destino', rename: 'Nuevo nombre base', trash: '¿Mover los archivos seleccionados a la papelera del servidor? Se quitarán de su ubicación actual y podrán restaurarse desde la papelera.', restore: '¿Restaurar el archivo en su ubicación original?', permanent: '¿Eliminar el archivo definitivamente?', empty: '¿Vaciar la papelera definitivamente?', password: 'Contraseña para confirmar', summary: 'Archivos / bytes', unavailable: 'La gestión de archivos ya no está disponible.', kept: 'Se eliminó la entrada del catálogo. El archivo permanece en el disco.', progress: 'Procesados', loading: 'Cargando…', files: '{n} archivos', file1: '1 archivo', hidden: ', {m} de ellos no visibles', more: '… y {n} más', cancelling: 'Cancelando…' },
  fr: { move: 'Déplacer', trashLabel: 'Mettre à la corbeille', target: 'Dossier de destination', rename: 'Nouveau nom de base', trash: 'Déplacer les fichiers sélectionnés dans la corbeille du serveur ? Ils seront retirés de leur emplacement actuel et pourront être restaurés depuis la corbeille.', restore: 'Restaurer le fichier à son emplacement initial ?', permanent: 'Supprimer définitivement le fichier ?', empty: 'Vider définitivement la corbeille ?', password: 'Mot de passe de confirmation', summary: 'Fichiers / octets', unavailable: 'La gestion des fichiers n’est plus disponible.', kept: 'L’entrée du catalogue a été supprimée. Le fichier reste sur le disque.', progress: 'Traités', loading: 'Chargement…', files: '{n} fichiers', file1: '1 fichier', hidden: ', dont {m} non visibles', more: '… et {n} autres', cancelling: 'Annulation…' },
};
type Action = 'rename' | 'move' | 'trash' | 'restore' | 'permanent' | 'empty';
export { webFileNameParts };

/** Web-only actions share one confirmation and never route through desktop handlers. */
export function WebFileActions({ fileIds = [], names, hiddenCount = 0, name, trash = false, empty = false, onDone, requestTrashConfirm = false, onConfirmationClosed }: {
  fileIds?: string[]; names?: string[]; hiddenCount?: number; name?: string; trash?: boolean; empty?: boolean; onDone?: () => void; requestTrashConfirm?: boolean; onConfirmationClosed?: () => void;
}) {
  const allowed = useCanManageFiles();
  const { lockProps } = useImportLock();
  const { language } = useLanguage();
  const t = useT();
  const c = copy[language];
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
  useEffect(() => { if (!allowed) setPassword(''); }, [allowed]);
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
    opener.current = button; ids.current = [...fileIds]; snapshotNames.current = { names: [...(names ?? [])], hidden: hiddenCount };
    setAction(value); setError(null); setNotice(null); setPassword(''); setTarget('');
    setSummary(null); setProgress(null); setBase(webFileNameParts(name ?? '').base);
  };
  const submit = async () => {
    if (!action || submitting.current || !canManageFiles() || lockProps.disabled) return;
    submitting.current = true; cancelRequested.current = false; setCancelling(false); setBusy(true); setError(null);
    const secret = password; setPassword('');
    try {
      if (action === 'rename') await fileOp('rename_file', { fileId: ids.current[0], name: base });
      if (action === 'move') await fileOpsBatch('move_files_to_folder', ids.current, { folderId: target }, () => cancelRequested.current);
      if (action === 'trash') await fileOpsBatch('delete_files', ids.current, {}, () => cancelRequested.current);
      if (action === 'restore') await fileOp('restore_file', { fileId: ids.current[0] });
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
  const actions: Action[] = empty ? ['empty'] : trash ? ['restore', 'permanent'] : [...(fileIds.length === 1 && name ? ['rename' as const] : []), 'move', 'trash'];
  return <div className="flex flex-wrap gap-2">
    {allowed && actions.map(value => <button key={value} type="button" className={catalogActionButton} disabled={busy || lockProps.disabled || (!empty && !fileIds.length)} aria-haspopup="dialog" onClick={event => open(value, event.currentTarget)}>{labels[value]}</button>)}
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
        {action === 'move' && <label className="block">{c.target}<select className="block w-full border p-2" value={target} disabled={busy || loading} onChange={e => setTarget(e.target.value)}><option value="">—</option>{folders.map(folder => <option key={folder.id} value={folder.id}>{folder.name} (#{folder.id})</option>)}</select></label>}
        {summary && <p>{c.summary}: {summary.count.toLocaleString(language)} / {summary.bytes.toLocaleString(language)}</p>}
        {(action === 'permanent' || action === 'empty') && !notice && <label className="block">{c.password}<input className="block w-full border p-2" type="password" autoComplete="new-password" data-1p-ignore data-lpignore="true" data-form-type="other" value={password} disabled={busy || !allowed} onChange={e => setPassword(e.target.value)} /></label>}
        {loading && <p role="status">{c.loading}</p>}
        {cancelling && <p role="status">{c.cancelling}</p>}
        {progress && <p role="status">{c.progress}: {progress[0]} / {progress[1]}</p>}
        {!allowed && <p role="alert">{c.unavailable}</p>}
        {error && <p role="alert">{error}</p>}
        {notice && <p role="status">{notice}</p>}
        <div className="flex justify-end gap-2">
          <button data-initial-focus type="button" className={catalogActionButton} disabled={cancelling} onClick={close}>{t('cancel')}</button>
          {!notice && <button type="button" className={catalogActionButton} disabled={busy || loading || !allowed || lockProps.disabled || (action === 'rename' && !base.trim()) || (action === 'move' && !target) || ((action === 'permanent' || action === 'empty') && !password) || (action === 'empty' && (!summary || summary.count === 0))} onClick={() => void submit()}>{labels[action]}</button>}
        </div>
      </div>
    </CatalogActionDialog>}
  </div>;
}
