import { useEffect, useRef, useState } from 'react';
import * as session from '../lib/webSession';
import { messageOf } from '../lib/errors';
import { useT } from '../i18n/LanguageContext';

/** Event the menu sends to `WebAuthGate`, which owns the password form. */
export const WEB_CHANGE_PASSWORD_EVENT = 'katalog-web-change-password';

/** Web only: password change and logout live in the header instead of floating over the content. */
export function WebAccountMenu() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false); };
    const esc = (event: KeyboardEvent) => { if (event.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', close);
    document.addEventListener('keydown', esc);
    return () => { document.removeEventListener('mousedown', close); document.removeEventListener('keydown', esc); };
  }, [open]);
  const itemClass = 'block w-full text-left px-3 py-2 text-body text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer disabled:opacity-50';
  return (
    <div ref={root} className="relative shrink-0">
      <button type="button" aria-haspopup="menu" aria-expanded={open} onClick={() => { setError(''); setOpen(o => !o); }}
        className="h-8 px-2 rounded border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)] text-body cursor-pointer hover:text-[var(--ink)]">{t('webAccount')}</button>
      {open && (
        <div role="menu" className="absolute right-0 top-10 z-50 min-w-[180px] py-1 rounded border border-[var(--line)] bg-[var(--panel)] shadow-lg">
          <button role="menuitem" className={itemClass} onClick={() => { setOpen(false); window.dispatchEvent(new Event(WEB_CHANGE_PASSWORD_EVENT)); }}>{t('webChangePassword')}</button>
          <button role="menuitem" className={itemClass} disabled={busy} onClick={async () => {
            setBusy(true); setError('');
            try { await session.logout(); setOpen(false); } catch (e) { setError(messageOf(e)); } finally { setBusy(false); }
          }}>{t('webLogout')}</button>
          {error && <p role="alert" className="px-3 py-2 text-small text-[var(--accent)]">{error}</p>}
        </div>
      )}
    </div>
  );
}
