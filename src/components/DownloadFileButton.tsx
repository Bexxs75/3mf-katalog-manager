import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { getSession } from '../lib/webSession';
import { messageOf } from '../lib/errors';

export function DownloadFileButton({ fileId, className, onSuccess }: {
  fileId: string; className?: string; onSuccess?: () => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const request = useRef(0);
  useEffect(() => {
    request.current++; setBusy(false); setError('');
    return () => { request.current++; };
  }, [fileId]);
  const isWeb = import.meta.env.VITE_TARGET === 'web';
  if (!isWeb) return null;
  return <div>
    <button disabled={busy}
      className={className ?? 'px-3 py-2 rounded-md border border-[var(--line)] text-small font-semibold text-[var(--ink-2)] hover:text-[var(--accent)] disabled:opacity-50'}
      onClick={async () => {
        const current = ++request.current;
        setBusy(true); setError('');
        try {
          // Publishing the fresh session makes WebAuthGate show login/password
          // change before any navigation. Never buffer model bytes in JavaScript.
          const session = await getSession();
          if (request.current !== current || !session.authenticated || session.mustChangePassword) return;
          const link = document.createElement('a');
          link.href = `/api/files/${encodeURIComponent(fileId)}/download`;
          link.download = '';
          document.body.append(link);
          link.click(); link.remove();
          onSuccess?.();
        } catch (e) {
          if (request.current === current) setError(messageOf(e));
        } finally {
          if (request.current === current) setBusy(false);
        }
      }}>{t('downloadModel')}</button>
    {error && <p role="alert" className="text-small text-[var(--accent)]">{error}</p>}
  </div>;
}
