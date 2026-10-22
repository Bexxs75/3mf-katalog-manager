import { useEffect, useState } from 'react';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import { Icon } from './Icon';

interface Props {
  outcome: { renamed: number; failed: number } | null;
  onClose: () => void;
}

const AUTO_HIDE_MS = 5000;

/** Result of a date rename. A clean result hides itself after a few seconds (hovering pauses it); a partial failure stays until dismissed so it is not missed. */
export function DateRenameBanner({ outcome, onClose }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const [hovered, setHovered] = useState(false);
  const shown = outcome !== null;
  const failed = (outcome?.failed ?? 0) > 0;
  useEffect(() => { if (!shown) setHovered(false); }, [shown]);
  useEffect(() => {
    if (!shown || failed || hovered) return;
    const timer = window.setTimeout(onClose, AUTO_HIDE_MS);
    return () => window.clearTimeout(timer);
  }, [shown, failed, hovered, outcome, onClose]);
  if (!outcome) return null;
  const text = [
    formatCount(t('dateRenameDone'), outcome.renamed),
    ...(failed ? [formatCount(t('dateRenameFailedCount'), outcome.failed)] : []),
  ].join(', ');
  return (
    <div role={failed ? 'alert' : 'status'}
      onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
      className={`pointer-events-auto max-w-[min(90vw,480px)] flex items-start gap-2.5 px-3.5 py-2.5 rounded-[4px] border bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] ${failed ? 'border-red-400 text-red-400' : 'border-[var(--line-strong)] text-[var(--ink)]'}`}>
      <p className="flex-1 min-w-0 break-words">{text}</p>
      <button type="button" onClick={onClose} aria-label={t('dateRenameNoticeDismiss')}
        className="flex-none w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
        <Icon name="close" size={14} />
      </button>
    </div>
  );
}
