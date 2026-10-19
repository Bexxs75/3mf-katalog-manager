import { displayName } from '../lib/displayName';
import { useT } from '../i18n/LanguageContext';
import { Icon } from './Icon';
import { useEffect } from 'react';
import type { FolderMoveBlock } from '../lib/folderTree';
import { ReportProblemLink } from '../diagnostics/ReportProblemLink';

const BLOCKED_MESSAGES: Record<FolderMoveBlock, 'folderMoveCatalogRoot' | 'folderMoveSelf' | 'folderMoveDescendant' | 'folderMoveAlreadyThere' | 'folderMoveNameTaken'> = {
  catalogRoot: 'folderMoveCatalogRoot',
  self: 'folderMoveSelf',
  descendant: 'folderMoveDescendant',
  alreadyThere: 'folderMoveAlreadyThere',
  nameTaken: 'folderMoveNameTaken',
};

// Errors carry long backend messages the user has to read in full, so they stay
// longer than the short success confirmations.
const SUCCESS_DURATION_MS = 3000;
const ERROR_DURATION_MS = 8000;

interface Props {
  from: string;
  to: string;
  collection?: 'added' | 'already';
  error?: boolean;
  /** A folder drop that was refused before reaching the backend; replaces `to` with a translated reason. */
  blocked?: FolderMoveBlock;
  /** Only meaningful together with `error`: offers "Report problem" for an unexpected error. */
  unexpected?: boolean;
  onDone: () => void;
}

/**
 * Short confirmation after a move by dragging or after creating a
 * folder; disappears after a few seconds (`onDone`), errors later. A new move restarts the
 * timer. With `error`, `to` carries the error message (no arrow, red accent).
 */
export function MoveToast({ from, to, error = false, unexpected = false, collection, blocked, onDone }: Props) {
  const t = useT();
  useEffect(() => {
    const timer = setTimeout(onDone, error ? ERROR_DURATION_MS : SUCCESS_DURATION_MS);
    return () => clearTimeout(timer);
  }, [from, to, error, collection, blocked, onDone]);

  return (
    <div
      className={`fixed bottom-4 left-1/2 -translate-x-1/2 z-50 max-w-[min(90vw,420px)] px-4 py-2.5 rounded-[6px] border text-small font-medium shadow-[var(--shadow)] flex ${error ? 'items-start' : 'items-center'} gap-1.5 ${
        error
          ? 'border-red-400 bg-[var(--panel)] text-red-400'
          : 'border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)]'
      }`}
      role="status"
    >
      {error ? (
        <>
          <span className="font-semibold flex-none mt-0.5"><Icon name="close" size={14} /></span>
          {/* Wraps instead of truncating: long paths without spaces break via break-words. */}
          <div className="min-w-0 break-words">
            <span className="font-semibold">{displayName(from)}:</span>{' '}
            <span className="opacity-90">{blocked ? t(BLOCKED_MESSAGES[blocked]) : displayName(to)}</span>
            {unexpected && <> <ReportProblemLink /></>}
          </div>
        </>
      ) : collection ? (
        <span>{t(collection === 'added' ? 'collectionDropAdded' : 'collectionDropAlready').replace('{name}', () => displayName(from)).replace('{collection}', () => displayName(to))}</span>
      ) : (
        <>
          <span className="font-semibold overflow-hidden text-ellipsis whitespace-nowrap">{displayName(from)}</span>
          <span className="opacity-70 flex-none">→</span>
          <span className="opacity-90 overflow-hidden text-ellipsis whitespace-nowrap">{displayName(to)}</span>
        </>
      )}
    </div>
  );
}
