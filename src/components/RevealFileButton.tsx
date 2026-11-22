import { isReadOnly } from '../lib/readOnly';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { useEffect, useRef, useState } from 'react';
import { invoke } from '../lib/transport';
import { useT } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { Icon } from './Icon';

/** Each reveal opens a file manager window, so a large selection is capped rather than flooding the desktop. */
export const MAX_REVEAL_WINDOWS = 5;

export function RevealFileButton({ fileId, fileIds, label, compact = false, showIcon = true, className, onSuccess }: {
  fileIds?: string[]; label?: string; fileId: string; compact?: boolean; showIcon?: boolean; className?: string; onSuccess?: () => void;
}) {
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const { lockProps } = useImportLock();
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const request = useRef(0);
  useEffect(() => {
    request.current++;
    setError(null); setNotice(null); setBusy(false);
    return () => { request.current++; };
  }, [fileId]);
  if (isReadOnly()) return null;
  if (container) return null;
  return <div className={compact ? 'flex-none' : undefined}>
    <button {...lockProps} disabled={lockProps.disabled || busy}
      title={t('showInFileManager')} aria-label={label ?? t('showInFileManager')}
      className={className ?? 'px-3 py-2 rounded-md border border-[var(--line)] text-small font-semibold text-[var(--ink-2)] hover:text-[var(--accent)] disabled:opacity-50'}
      onClick={async () => {
        const current = ++request.current;
        setError(null); setNotice(null); setBusy(true);
        try {
          const all = fileIds ?? [fileId];
          const ids = all.slice(0, MAX_REVEAL_WINDOWS);
          if (all.length > ids.length) setNotice(t('revealLimitNotice').replace('{max}', String(MAX_REVEAL_WINDOWS)).replace('{skipped}', String(all.length - ids.length)));
          // One failing model must not stop the others; the first error is still reported.
          let firstError: unknown = null;
          for (const id of ids) {
            try { await invoke('reveal_in_file_manager', { fileId: id }); }
            catch (e) { firstError ??= e; }
          }
          if (firstError) throw firstError;
          // Past the cap the notice has to stay readable, so the surrounding menu is not closed.
          if (request.current === current && all.length <= MAX_REVEAL_WINDOWS) onSuccess?.();
        } catch (e) {
          if (request.current === current) setError(toAppError(e));
        } finally {
          if (request.current === current) setBusy(false);
        }
      }}>
      {showIcon && <Icon name="folder" size={16} />}{!compact && <span className={showIcon ? 'ml-2' : undefined}>{label ?? t('showInFileManager')}</span>}
    </button>
    {notice && <p role="status" className="text-small text-[var(--ink-2)]">{notice}</p>}
    {error && <p role="alert" className="text-small text-[var(--accent)]"><ErrorText error={error} /></p>}
  </div>;
}
