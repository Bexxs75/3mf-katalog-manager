import { useRef, useState } from 'react';
import { canEditMetadata, useCanEditMetadata } from '../lib/webSession';
import { useT } from '../i18n/LanguageContext';
import { displayName } from '../lib/displayName';
import { messageOf } from '../lib/errors';
import { reportMetadataError } from '../lib/metadataErrors';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';

/** A catalog-only action; file deletion is available solely through upload discard. */
export function WebRemoveFromCatalogButton({ name, onRemove, onRemoved, className = catalogActionButton }: {
  name: string; onRemove?: () => Promise<void>; onRemoved?: () => void; className?: string;
}) {
  const allowed = useCanEditMetadata();
  const t = useT();
  const button = useRef<HTMLButtonElement>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (import.meta.env.VITE_TARGET !== 'web' || !allowed || !onRemove) return null;
  const close = () => { if (!busy) setConfirmRemove(false); };
  return <>
    <button type="button" ref={button} className={`${className} whitespace-nowrap`} aria-haspopup="dialog" onClick={() => { setError(null); setConfirmRemove(true); }}>{t('removeCatalog')}</button>
    {confirmRemove && <CatalogActionDialog title={t('removeModelQuestion').replace('{name}', displayName(name))} onClose={close} returnFocus={button}>
      <p>{t('webRemoveModelHint')}</p>
      {error && <p role="alert">{error}</p>}
      <div className="flex gap-2 justify-end">
        <button type="button" data-initial-focus className={catalogActionButton} disabled={busy} onClick={close}>{t('cancel')}</button>
        <button type="button" className={catalogActionButton} disabled={busy} onClick={async () => {
          if (busy || !canEditMetadata()) return;
          setBusy(true); setError(null);
          try { await onRemove(); setConfirmRemove(false); onRemoved?.(); }
          catch (reason) { reportMetadataError(reason); setError(messageOf(reason)); }
          finally { setBusy(false); }
        }}>{t('removeEntry')}</button>
      </div>
    </CatalogActionDialog>}
  </>;
}
