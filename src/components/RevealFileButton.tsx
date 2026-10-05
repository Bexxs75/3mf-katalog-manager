import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { Icon } from './Icon';

export function RevealFileButton({ fileId, compact = false, className, onSuccess }: {
  fileId: string; compact?: boolean; className?: string; onSuccess?: () => void;
}) {
  const t = useT();
  const { lockProps } = useImportLock();
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const request = useRef(0);
  useEffect(() => {
    request.current++;
    setError(null); setBusy(false);
    return () => { request.current++; };
  }, [fileId]);
  return <div className={compact ? 'flex-none' : undefined}>
    <button {...lockProps} disabled={lockProps.disabled || busy}
      title={t('showInFileManager')} aria-label={t('showInFileManager')}
      className={className ?? 'px-3 py-2 rounded-md border border-[var(--line)] text-small font-semibold text-[var(--ink-2)] hover:text-[var(--accent)] disabled:opacity-50'}
      onClick={async () => {
        const current = ++request.current;
        setError(null); setBusy(true);
        try {
          await invoke('reveal_in_file_manager', { fileId });
          if (request.current === current) onSuccess?.();
        } catch (e) {
          if (request.current === current) setError(toAppError(e));
        } finally {
          if (request.current === current) setBusy(false);
        }
      }}>
      <Icon name="folder" size={16} />{!compact && <span className="ml-2">{t('showInFileManager')}</span>}
    </button>
    {error && <p role="alert" className="text-small text-[var(--accent)]"><ErrorText error={error} /></p>}
  </div>;
}
