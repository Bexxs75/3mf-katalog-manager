import { useState, type CSSProperties, type ReactNode } from 'react';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { useT } from '../i18n/LanguageContext';

export function ExternalLink({ url, label, onOpen, children, className, style }: {
  url: string; label: string; onOpen?: () => void; children?: ReactNode;
  className?: string; style?: CSSProperties;
}) {
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const [result, setResult] = useState<{ url: string; ok: boolean } | null>(null);
  if (!container) return onOpen
    ? <button type="button" onClick={onOpen} className={className} style={style}>{children ?? label}</button>
    : <a href={url} target="_blank" rel="noreferrer" className={className} style={style}>{children ?? label}</a>;

  const copy = async () => {
    setResult(null);
    try {
      await navigator.clipboard.writeText(url);
      setResult({ url, ok: true });
    } catch {
      setResult({ url, ok: false });
    }
  };
  return <span className="inline-flex min-w-0 max-w-full flex-col items-start gap-1 text-small">
    <span className="max-w-full break-all" style={{ userSelect: 'text' }}>{url}</span>
    <button type="button" onClick={() => void copy()} aria-label={t('externalLinkCopyLabel').replace('{target}', label)}
      className="px-2 py-1 border border-[var(--line-strong)] rounded cursor-pointer hover:text-[var(--accent)]">
      {t('externalLinkCopy')}
    </button>
    <span role="status" aria-live="polite" className="text-caption">
      {result?.url === url ? t(result.ok ? 'externalLinkCopied' : 'externalLinkCopyFailed') : ''}
    </span>
  </span>;
}
