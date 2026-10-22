import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import * as api from '../lib/api/linuxMenu';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';

/** One-time question on the first start of a Linux AppImage; nothing is written before "Yes". */
export function LinuxMenuPrompt({ wait = false }: { wait?: boolean } = {}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [added, setAdded] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const mountedRef = useRef(true);
  // Set to true again on setup: StrictMode runs setup, cleanup, setup, and a
  // cleanup-only reset would leave the ref false for the real mount.
  useEffect(() => { mountedRef.current = true; return () => { mountedRef.current = false; }; }, []);
  // `wait` holds the question back while another first-run dialog is up.
  useEffect(() => {
    if (wait) return;
    api.getLinuxMenuStatus().then(s => { if (mountedRef.current && s?.shouldPrompt) setOpen(true); }).catch(() => {});
  }, [wait]);
  // The answer buttons are replaced once the entry is added; without this the
  // focused button disappears and keyboard focus falls back to the page.
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => { if (added) closeRef.current?.focus(); }, [added]);
  if (!open) return null;
  async function answer(choice: api.LinuxMenuAnswer) {
    setBusy(true);
    setError(null);
    try {
      const next = await api.answerLinuxMenuPrompt(choice);
      if (!mountedRef.current) return;
      if (choice === 'yes' && next?.installed) setAdded(true);
      else setOpen(false);
    } catch (e) {
      if (mountedRef.current) setError(toAppError(e));
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  }
  return <CatalogActionDialog title={added ? t('linuxMenuAddedTitle') : t('linuxMenuPromptTitle')} onClose={() => { if (busy) return; if (added) setOpen(false); else void answer('not_now'); }}>
    {added ? <>
      <p role="status">{t('linuxMenuAdded')}</p>
      <div className="flex flex-wrap gap-2">
        <button ref={closeRef} className={catalogActionButton} data-initial-focus onClick={() => setOpen(false)}>{t('impClose')}</button>
      </div>
    </> : <>
      <p>{t('linuxMenuPromptText')}</p>
      {error && <div role="alert" className="p-3 border border-[var(--crit)] rounded text-body">{t('linuxMenuFailed')} <ErrorText error={error} /></div>}
      <div className="flex flex-wrap gap-2">
        <button className={catalogActionButton} data-initial-focus disabled={busy} onClick={() => void answer('yes')}>{t('linuxMenuYes')}</button>
        <button className={catalogActionButton} disabled={busy} onClick={() => void answer('not_now')}>{t('linuxMenuNotNow')}</button>
        <button className={catalogActionButton} disabled={busy} onClick={() => void answer('never')}>{t('linuxMenuNever')}</button>
      </div>
    </>}
  </CatalogActionDialog>;
}
