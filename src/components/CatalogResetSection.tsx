import { Icon } from './Icon';
import { useImportLock } from '../hooks/ImportLockContext';
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';

export function CatalogResetSection({ modelCount, folderCount, onExport, onReset, backupError }: {
  modelCount: number; folderCount: number; onExport: () => Promise<boolean>;
  onReset: () => void; backupError?: AppError | null;
}) {
  const { lockProps } = useImportLock();
  const t = useT();
  const formatCount = useFormatCount();
  const [open, setOpen] = useState(false);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  return <section className="mt-4">
    <h3 className="text-[length:var(--font-size-body)] font-semibold mb-2">{t('resetCatalog')}</h3>
    <button {...lockProps} className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-7 w-full rounded-[3px] border border-dashed border-[var(--crit)] bg-transparent text-[var(--crit)] text-small cursor-pointer`} onClick={() => { setSaved(false); setError(null); setOpen(true); }}>{t('resetCatalog')} …</button>
    <p className="text-[length:var(--font-size-meta)] text-[var(--ink-3)] mt-1.5">{t('resetCatalogHint')}</p>
    {open && <CatalogActionDialog title={t('resetCatalogQuestion')} onClose={() => { if (!busy) setOpen(false); }}>
      <p>{t('resetCatalogDetails').replace('{models}', formatCount(t('modelCountLabel'), modelCount)).replace('{folders}', formatCount(t('folderCountLabel'), folderCount))}</p>
      <ul className="text-[var(--good)] space-y-1">
        <li><Icon name="check" size={14} /> {t('resetSafeFiles')}</li><li><Icon name="check" size={14} /> {t('resetSafeInventory')}</li><li><Icon name="check" size={14} /> {t('resetSafeTrash')}</li>
      </ul>
      <p>{t('resetBackupTip')}</p>
      {(error ?? backupError) && <div role="alert"><ErrorText error={(error ?? backupError)!} /></div>}
      <div className="flex flex-wrap gap-2 justify-end">
        <button className={`${catalogActionButton} mr-auto`} disabled={saved || busy} onClick={async () => {
          setBusy(true); setError(null);
          try { setSaved(await onExport()); } catch (e) { setError(toAppError(e)); }
          finally { setBusy(false); }
        }}>{saved && <Icon name="check" size={14} />}{saved ? t('resetBackupDone') : t('resetBackupFirst')}</button>
        <button data-initial-focus className={catalogActionButton} disabled={busy} onClick={() => setOpen(false)}>{t('cancel')}</button>
        <button className={`${catalogActionBase} hover:bg-[var(--panel-2)] hover:text-[var(--ink)] border-[var(--crit)] bg-[var(--crit)] text-white`} disabled={busy} {...lockProps} onClick={async () => {
          setBusy(true); setError(null);
          try { await invoke('reset_catalog'); setOpen(false); onReset(); }
          catch (e) { setError(toAppError(e)); }
          finally { setBusy(false); }
        }}>{t('resetConfirm')}</button>
      </div>
    </CatalogActionDialog>}
  </section>;
}
