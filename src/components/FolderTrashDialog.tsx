import { useEffect, useState, type RefObject } from 'react';
import { displayName } from '../lib/displayName';
import { useImportLock } from '../hooks/ImportLockContext';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { folderTrashSummary, trashFolder } from '../lib/api/folders';
import { isReadOnly } from '../lib/readOnly';
import type { FolderTrashResult, FolderTrashSummary } from '../types';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';

/** Confirmation for moving a folder's models to the app trash; the summary is loaded when the dialog opens. */
export function FolderTrashDialog({ folderId, name, onClose, onTrashed, returnFocus }: {
  folderId: string; name: string;
  onClose: () => void; onTrashed: (result: FolderTrashResult) => void; returnFocus: RefObject<HTMLElement | null>;
}) {
  const { lockProps } = useImportLock();
  const t = useT();
  const formatCount = useFormatCount();
  const [summary, setSummary] = useState<FolderTrashSummary | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  useEffect(() => {
    let cancelled = false;
    folderTrashSummary(folderId).then((value) => {
      if (!cancelled) setSummary(value);
    }).catch((e) => { if (!cancelled) setError(toAppError(e)); });
    return () => { cancelled = true; };
  }, [folderId]);
  const blocked = summary?.isCatalogRootOrAncestor === true;
  return <CatalogActionDialog title={t('trashFolderQuestion').replace('{name}', displayName(summary?.name ?? name))} onClose={() => { if (!busy) onClose(); }} returnFocus={returnFocus}>
    {blocked
      ? <p>{t('trashFolderProtected')}</p>
      : summary && <>
        <ul className="list-disc pl-5 space-y-1">
          <li>{formatCount(t('trashFolderSubfolders'), summary.subfolderCount)}</li>
          <li>{formatCount(t('trashFolderModels'), summary.modelCount)}</li>
          {summary.otherFileCount > 0 && <li>{formatCount(t(summary.otherFileCountTruncated ? 'trashFolderOthersAtLeast' : 'trashFolderOthers'), summary.otherFileCount)}</li>}
        </ul>
        <p>{t(isReadOnly() ? 'trashFolderWebNote' : 'trashFolderEmptyNote')}</p>
      </>}
    {progress && progress.total > 0 && <p role="status">{t('trashFolderProgress').replace('{done}', String(progress.done)).replace('{total}', String(progress.total))}</p>}
    {error && <div role="alert"><ErrorText error={error} /></div>}
    <div className="flex gap-2 justify-end">
      <button data-initial-focus className={catalogActionButton} disabled={busy} onClick={onClose}>{t('cancel')}</button>
      {!blocked && <button className={`${catalogActionBase} border-red-500 bg-red-600 text-white hover:bg-red-700`} disabled={busy || !summary} {...lockProps} onClick={async () => {
        setBusy(true); setError(null);
        try { const result = await trashFolder(folderId, (done, total) => setProgress({ done, total })); onTrashed(result); onClose(); }
        catch (e) { setError(toAppError(e)); setBusy(false); }
      }}>{t('trashFolderConfirm')}</button>}
    </div>
  </CatalogActionDialog>;
}
