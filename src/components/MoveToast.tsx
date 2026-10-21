import { displayName } from '../lib/displayName';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { isMacPlatform } from '../lib/keyboardGuard';
import type { MoveBatchInfo, UndoOutcome } from '../lib/moveUndo';
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
// Long enough to find the button, read the hint and decide.
const UNDO_DURATION_MS = 15000;

type ReasonKey = 'moveReasonSourceMissing' | 'moveReasonFolderMissing' | 'moveReasonGone' | 'moveReasonSensitive' | 'moveReasonError' | 'undoReasonNameExists';
const REASONS: Record<string, ReasonKey> = {
  source_missing: 'moveReasonSourceMissing',
  target_missing: 'moveReasonFolderMissing',
  invalid_id: 'moveReasonGone',
  not_found: 'moveReasonGone',
  in_trash: 'moveReasonGone',
  sensitive_path: 'moveReasonSensitive',
  error: 'moveReasonError',
  name_exists: 'undoReasonNameExists',
};
const baseName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

interface Props {
  from: string;
  to: string;
  collection?: 'added' | 'already';
  error?: boolean;
  /** A folder drop that was refused before reaching the backend; replaces `to` with a translated reason. */
  blocked?: FolderMoveBlock;
  /** Only meaningful together with `error`: offers "Report problem" for an unexpected error. */
  unexpected?: boolean;
  /** Result of moving files (one or many), replaces the plain "from → to" line. */
  batch?: MoveBatchInfo;
  /** Result of an undo step. */
  undo?: UndoOutcome;
  /** Shows the "Undo" button; the undo itself is the caller's. */
  onUndo?: () => void;
  onDone: () => void;
}

/** The "Undo" button with its shortcut hint, shared by the toasts that can be undone. */
export function UndoControl({ onUndo }: { onUndo: () => void }) {
  const t = useT();
  const shortcut = `${isMacPlatform() ? 'Cmd' : t('keyboardCtrl')}+Z`;
  return (
    <>
      <button type="button" onClick={onUndo} aria-keyshortcuts={isMacPlatform() ? 'Meta+Z' : 'Control+Z'}
        className="flex-none ml-1 px-2 h-6 rounded-[3px] border border-current font-semibold cursor-pointer hover:bg-[var(--bg)] hover:text-[var(--ink)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]">
        {t('undoButton')}
      </button>
      <kbd aria-hidden="true" className="flex-none opacity-70 font-medium tabular-nums text-caption">{shortcut}</kbd>
    </>
  );
}

/**
 * Short confirmation after a move by dragging or after creating a
 * folder; disappears after a few seconds (`onDone`), errors later. A new move restarts the
 * timer. With `error`, `to` carries the error message (no arrow, red accent).
 */
export function MoveToast({ from, to, error = false, unexpected = false, collection, blocked, batch, undo, onUndo, onDone }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const failed = !!batch && batch.summary.failed > 0;
  // Partial failures and refused undo steps interrupt (alert); plain errors keep their long-standing status role.
  const alerting = failed || (!!undo && !undo.ok);
  const duration = onUndo ? UNDO_DURATION_MS : error || alerting ? ERROR_DURATION_MS : SUCCESS_DURATION_MS;
  useEffect(() => {
    const timer = setTimeout(onDone, duration);
    return () => clearTimeout(timer);
  }, [from, to, error, collection, blocked, batch, undo, duration, onDone]);

  const reasonText = (reason?: string) => reason ? (REASONS[reason] ? t(REASONS[reason]) : reason) : t('moveReasonError');
  const withCount = (text: string, count: number) => formatCount({ one: text, other: text }, count);
  const fill = (text: string, values: Record<string, string>) =>
    text.replace(/\{(\w+)\}/g, (match, key: string) => values[key] ?? match);
  let message: string | null = null;
  let names: string | null = null;
  if (batch) {
    const { summary } = batch;
    const target = displayName(baseName(to));
    if (summary.moved > 0 && summary.skippedNameExists + summary.skippedAlreadyThere + summary.failed === 0) {
      message = fill(formatCount(t('moveBatchDone'), summary.moved), { names: displayName(batch.names[0] ?? ''), target });
    } else {
      const parts = [summary.moved > 0 ? withCount(t('moveBatchMoved'), summary.moved) : t('moveBatchNothing')];
      for (const [count, key] of [[summary.skippedNameExists, 'moveBatchSkippedExists'], [summary.skippedAlreadyThere, 'moveBatchSkippedThere'], [summary.failed, 'moveBatchFailed']] as const) {
        if (count > 0) parts.push(withCount(t(key), count));
      }
      message = `${parts.join(', ')}${summary.failed > 0 ? ` (${reasonText(summary.firstFailReason)})` : ''}.`;
    }
    if (batch.total > 1) names = `${batch.names.map(displayName).join(', ')}${batch.total > batch.names.length ? ', …' : ''}`;
  } else if (undo) {
    if (undo.kind === 'files') {
      message = undo.ok ? formatCount(t('undoDoneFiles'), undo.count ?? 0)
        : (undo.count ?? 0) > 0
          ? fill(t('undoFilesPartial'), { done: String(undo.count), total: String(undo.total ?? 0), reason: reasonText(undo.reason) })
          : fill(t('undoFailed'), { reason: reasonText(undo.reason) });
    } else if (!undo.ok) {
      message = fill(t('undoFailed'), { reason: reasonText(undo.reason) });
    } else if (undo.kind === 'folderMove') {
      message = fill(t('undoDoneFolderMove'), { name: displayName(undo.name ?? '') });
    } else {
      message = fill(t('undoDoneRename'), { from: displayName(undo.name ?? ''), to: displayName(undo.restoredName ?? '') });
    }
  }
  const undoButton = onUndo && <UndoControl onUndo={onUndo} />;

  return (
    <div
      className={`fixed bottom-4 left-1/2 -translate-x-1/2 z-50 max-w-[min(90vw,420px)] px-4 py-2.5 rounded-[6px] border text-small font-medium shadow-[var(--shadow)] flex ${error ? 'items-start' : 'items-center'} gap-1.5 ${
        error
          ? 'border-red-400 bg-[var(--panel)] text-red-400'
          : failed
            ? 'border-red-400 bg-[var(--panel)] text-red-400'
            : 'border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)]'
      }`}
      role={alerting ? 'alert' : 'status'}
    >
      {message ? (
        <>
          <div className="min-w-0 break-words">
            <span>{message}</span>
            {names && <span className="block opacity-80 font-normal">{names}</span>}
          </div>
          {undoButton}
        </>
      ) : error ? (
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
          {undoButton}
        </>
      )}
    </div>
  );
}
