import { Icon } from './Icon';
import { useEffect } from 'react';
import { ReportProblemLink } from '../diagnostics/ReportProblemLink';

interface Props {
  from: string;
  to: string;
  error?: boolean;
  /** Only meaningful together with `error`: offers "Report problem" for an unexpected error. */
  unexpected?: boolean;
  onDone: () => void;
}

/**
 * Short confirmation after a move by dragging or after creating a
 * folder; disappears after 3 s (`onDone`). A new move restarts the
 * timer. With `error`, `to` carries the error message (no arrow, red accent).
 */
export function MoveToast({ from, to, error = false, unexpected = false, onDone }: Props) {
  useEffect(() => {
    const timer = setTimeout(onDone, 3000);
    return () => clearTimeout(timer);
  }, [from, to, error, onDone]);

  return (
    <div
      className={`fixed bottom-4 left-1/2 -translate-x-1/2 z-50 max-w-[min(90vw,420px)] px-4 py-2.5 rounded-[6px] border text-small font-medium shadow-[var(--shadow)] flex items-center gap-1.5 ${
        error
          ? 'border-red-400 bg-[var(--panel)] text-red-400'
          : 'border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)]'
      }`}
      role="status"
    >
      {error ? (
        <>
          <span className="font-semibold flex-none"><Icon name="close" size={14} /></span>
          <span className="font-semibold overflow-hidden text-ellipsis whitespace-nowrap">{from}:</span>
          <span className="opacity-90 overflow-hidden text-ellipsis whitespace-nowrap">{to}</span>
          {unexpected && <ReportProblemLink />}
        </>
      ) : (
        <>
          <span className="font-semibold overflow-hidden text-ellipsis whitespace-nowrap">{from}</span>
          <span className="opacity-70 flex-none">→</span>
          <span className="opacity-90 overflow-hidden text-ellipsis whitespace-nowrap">{to}</span>
        </>
      )}
    </div>
  );
}
