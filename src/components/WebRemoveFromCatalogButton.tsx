import { useRef, useState, type ReactNode } from 'react';
import { canEditMetadata, useCanEditMetadata } from '../lib/webSession';
import { useT } from '../i18n/LanguageContext';
import { displayName } from '../lib/displayName';
import { messageOf } from '../lib/errors';
import { reportMetadataError } from '../lib/metadataErrors';
import { REMOVAL_PASSWORD_FROM } from '../lib/api/diskSync';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';

/** A catalog-only action; file deletion is available solely through upload discard. */
export function WebRemoveFromCatalogButton({ count = 1, menuLabel, menuShortcut, name, onRemove, onRemoved, className = catalogActionButton }: {
  count?: number; menuLabel?: ReactNode; menuShortcut?: string; name: string; onRemove?: (password?: string) => Promise<void>; onRemoved?: () => void; className?: string;
}) {
  const allowed = useCanEditMetadata();
  const t = useT();
  const button = useRef<HTMLButtonElement>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [password, setPassword] = useState('');
  // The server asks for the password when one call removes this many entries or more.
  const needsPassword = count >= REMOVAL_PASSWORD_FROM;
  if (import.meta.env.VITE_TARGET !== 'web' || !allowed || !onRemove) return null;
  const close = () => { if (!busy) { setConfirmRemove(false); setPassword(''); } };
  return <>
    <button type="button" ref={button} className={`${className} whitespace-nowrap`} aria-haspopup="dialog" aria-keyshortcuts={menuShortcut} onClick={() => { setError(null); setConfirmRemove(true); }}>{menuLabel ?? t('removeCatalog')}</button>
    {confirmRemove && <CatalogActionDialog title={count > 1 ? t('modelMenuRemoveQuestion').replace('{count}', String(count)) : t('removeModelQuestion').replace('{name}', displayName(name))} onClose={close} returnFocus={button}>
      <p>{t('webRemoveModelHint')}</p>
      {needsPassword && <div><p>{t('diskApplyPasswordHint').replace('{n}', String(count))}</p><label className="block">{t('webFilePassword')}<input type="password" autoComplete="new-password" data-1p-ignore data-lpignore="true" className="block w-full border p-2 bg-[var(--panel)]" value={password} disabled={busy} onChange={event => setPassword(event.target.value)} /></label></div>}
      {error && <p role="alert">{error}</p>}
      <div className="flex gap-2 justify-end">
        <button type="button" data-initial-focus className={catalogActionButton} disabled={busy} onClick={close}>{t('cancel')}</button>
        <button type="button" className={catalogActionButton} disabled={busy || (needsPassword && !password)} onClick={async () => {
          if (busy || !canEditMetadata() || (needsPassword && !password)) return;
          setBusy(true); setError(null);
          const secret = password; setPassword('');
          try { await (needsPassword ? onRemove(secret) : onRemove()); setConfirmRemove(false); onRemoved?.(); }
          catch (reason) { reportMetadataError(reason); setError(messageOf(reason)); }
          finally { setBusy(false); }
        }}>{t('removeEntry')}</button>
      </div>
    </CatalogActionDialog>}
  </>;
}
