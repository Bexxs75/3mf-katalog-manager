import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { getSession } from '../lib/webSession';
import { toAppError, type AppError } from '../lib/errors';
import { formatAppError } from '../lib/errorMessage';

/** Browsers drop or block downloads fired in the same tick, so they are started one after another. */
export const DOWNLOAD_GAP_MS = 300;

export function DownloadFileButton({ fileId, fileIds, label, className, onSuccess }: {
  fileIds?: string[]; label?: string; fileId: string; className?: string; onSuccess?: () => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  // Kept as the error itself, not as finished text, so it follows a language change.
  const [error, setError] = useState<AppError | null>(null);
  const request = useRef(0);
  const unmounted = useRef(false);
  useEffect(() => { unmounted.current = false; return () => { unmounted.current = true; }; }, []);
  // Only a new request or another file overtakes a running download series; closing the menu that hosts
  // the button must not cut it short.
  useEffect(() => {
    request.current++; setBusy(false); setError(null);
  }, [fileId]);
  const isWeb = import.meta.env.VITE_TARGET === 'web';
  if (!isWeb) return null;
  return <div>
    <button disabled={busy}
      className={className ?? 'px-3 py-2 rounded-md border border-[var(--line)] text-small font-semibold text-[var(--ink-2)] hover:text-[var(--accent)] disabled:opacity-50'}
      onClick={async () => {
        const current = ++request.current;
        setBusy(true); setError(null);
        try {
          // Publishing the fresh session makes WebAuthGate show login/password
          // change before any navigation. Never buffer model bytes in JavaScript.
          const session = await getSession();
          if (unmounted.current || request.current !== current || !session.authenticated || session.mustChangePassword) return;
          const ids = fileIds ?? [fileId];
          for (let i = 0; i < ids.length; i++) {
            if (i > 0) {
              await new Promise(resolve => setTimeout(resolve, DOWNLOAD_GAP_MS));
              if (request.current !== current) return;
            }
            const link = document.createElement('a');
            link.href = `/api/files/${encodeURIComponent(ids[i])}/download`;
            link.download = '';
            document.body.append(link);
            link.click(); link.remove();
          }
          onSuccess?.();
        } catch (e) {
          if (request.current === current) setError(toAppError(e));
        } finally {
          if (request.current === current) setBusy(false);
        }
      }}>{label ?? t('downloadModel')}</button>
    {error && <p role="alert" className="text-small text-[var(--accent)]">{formatAppError(t, error)}</p>}
  </div>;
}
