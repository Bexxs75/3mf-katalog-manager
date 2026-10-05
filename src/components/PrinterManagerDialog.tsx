import { useState, type ReactNode } from 'react';
import { useModalDialog } from '../hooks/useModalDialog';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';

export const pmButton = 'px-3 py-1.5 rounded border border-[var(--line-strong)] text-small cursor-pointer hover:border-[var(--accent)] focus-visible:outline-2 focus-visible:outline-[var(--accent)] disabled:opacity-50';
export const pmField = 'w-full min-w-0 px-2 py-1.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/** Mounted only while open, so draft and error state never leak to another printer. */
export function PrinterManagerDialog({ title, children, onClose, onSubmit, submitLabel }: {
  title: string; children: ReactNode; onClose: () => void;
  onSubmit: () => Promise<unknown>; submitLabel: string;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const ref = useModalDialog({ open: true, onClose, busy });
  return <div data-modal-backdrop className="fixed inset-0 z-50 bg-black/45 grid place-items-center p-4">
    <div ref={ref} role="dialog" aria-modal="true" aria-label={title} tabIndex={-1}
      className="w-full max-w-xl max-h-[90vh] overflow-y-auto p-5 rounded-lg bg-[var(--panel)] border border-[var(--line)] shadow-[var(--shadow)]">
      <form onSubmit={async e => {
        e.preventDefault();
        if (busy) return;
        setBusy(true); setError(null);
        try { await onSubmit(); onClose(); } catch (e) { setError(toAppError(e)); } finally { setBusy(false); }
      }}>
        <h2 className="font-bold mb-4">{title}</h2>
        <fieldset disabled={busy} className="flex flex-col gap-3">{children}</fieldset>
        {error && <div role="alert" className="text-[var(--crit)] mt-3"><ErrorText error={error} /></div>}
        <div className="flex gap-2 justify-end mt-4">
          <button type="button" className={pmButton} disabled={busy} onClick={onClose}>{t('printersCancel')}</button>
          <button className={pmButton} disabled={busy}>{submitLabel}</button>
        </div>
      </form>
    </div>
  </div>;
}
