import { useImportLock } from '../hooks/ImportLockContext';
import { useEffect, useRef, useState, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';

interface Summary { name: string; subfolderCount: number; modelCount: number }
export function FolderCatalogMenu({ folderId, name, x, y, onClose, onRemoved, returnFocus }: {
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
  if (!dialog) return createPortal(
    <div ref={menu} role="menu" className="fixed z-50 min-w-[200px] overflow-hidden rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-lg"
      style={{ left: Math.max(0, Math.min(x, window.innerWidth - 260)), top: Math.max(0, Math.min(y, window.innerHeight - 50)) }}>
      <button {...lockProps} role="menuitem" className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)] focus-visible:bg-[var(--panel-2)] outline-none" onClick={() => setDialog(true)}>{t('removeCatalog')}</button>
    </div>, document.body,
  );
  return <CatalogActionDialog title={t('removeFolderQuestion').replace('{name}', summary?.name ?? name)} onClose={() => { if (!busy) onClose(); }} returnFocus={returnFocus}>
    {summary && <ul className="list-disc pl-5 space-y-1">
      <li>{formatCount(t('removeFolderList'), summary.subfolderCount).replace('{name}', summary.name)}</li>
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
