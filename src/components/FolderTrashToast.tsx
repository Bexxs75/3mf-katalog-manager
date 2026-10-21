import { useEffect } from 'react';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import type { FolderTrashResult } from '../types';

// Longer than the rename toast: partial results carry more to read.
const DURATION_MS = 8000;
const NAMES_SHOWN = 2;

const lastSegment = (path: string) => path.split(/[/\\]/).filter(Boolean).pop() ?? path;

/** "(a, b, …)" with at most two names, so a long list cannot blow up the toast. */
function nameList(paths: string[]) {
  const shown = paths.slice(0, NAMES_SHOWN).map(lastSegment).join(', ');
  return ` (${shown}${paths.length > NAMES_SHOWN ? ', …' : ''})`;
}

/** Outcome of moving a folder to the app trash, including folders and files that could not follow; disappears by itself (`onDone`). */
export function FolderTrashToast({ result, onDone }: { result: FolderTrashResult; onDone: () => void }) {
  const t = useT();
  const formatCount = useFormatCount();
  useEffect(() => {
    const timer = setTimeout(onDone, DURATION_MS);
    return () => clearTimeout(timer);
  }, [result, onDone]);

  const parts = [formatCount(t('trashFolderDone'), result.trashedModelCount)];
  if (result.keptOnDisk.length > 0) parts.push(formatCount(t('trashFolderKept'), result.keptOnDisk.length) + nameList(result.keptOnDisk));
  if (result.failed.length > 0) parts.push(formatCount(t('trashFolderFailed'), result.failed.length) + nameList(result.failed.map((f) => f.path)));
  return (
    <div
      className="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 max-w-[min(90vw,420px)] px-4 py-2.5 rounded-[6px] border border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)] text-small font-medium shadow-[var(--shadow)] break-words"
      role="status"
    >
      {parts.join('. ')}.
    </div>
  );
}
