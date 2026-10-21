import { displayName } from '../lib/displayName';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { useEffect } from 'react';
import { UndoControl } from './MoveToast';

const DURATION_MS = 4000;
// Long enough to find the button and decide.
const UNDO_DURATION_MS = 15000;

interface Props {
  from: string;
  to: string;
  count: number;
  onUndo?: () => void;
  onDone: () => void;
}

/** Short confirmation after a folder was renamed; disappears by itself (`onDone`). */
export function RenameToast({ from, to, count, onUndo, onDone }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const undoable = !!onUndo;
  useEffect(() => {
    const timer = setTimeout(onDone, undoable ? UNDO_DURATION_MS : DURATION_MS);
    return () => clearTimeout(timer);
  }, [from, to, count, undoable, onDone]);

  // One pass with a function replacer: names containing "{to}" or "$&" stay literal.
  const text = formatCount(t('folderRenamed'), count)
    .replace(/\{(from|to)\}/g, (_, key: string) => displayName(key === 'from' ? from : to));
  return (
    <div
      className="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 max-w-[min(90vw,420px)] px-4 py-2.5 rounded-[6px] border border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)] text-small font-medium shadow-[var(--shadow)] flex items-center gap-1.5"
      role="status"
    >
      <span className="min-w-0 break-words">{text}</span>
      {onUndo && <UndoControl onUndo={onUndo} />}
    </div>
  );
}
