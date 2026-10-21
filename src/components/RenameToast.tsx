import { displayName } from '../lib/displayName';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { useEffect } from 'react';

const DURATION_MS = 4000;

interface Props {
  from: string;
  to: string;
  count: number;
  onDone: () => void;
}

/** Short confirmation after a folder was renamed; disappears by itself (`onDone`). */
export function RenameToast({ from, to, count, onDone }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  useEffect(() => {
    const timer = setTimeout(onDone, DURATION_MS);
    return () => clearTimeout(timer);
  }, [from, to, count, onDone]);

  // One pass with a function replacer: names containing "{to}" or "$&" stay literal.
  const text = formatCount(t('folderRenamed'), count)
    .replace(/\{(from|to)\}/g, (_, key: string) => displayName(key === 'from' ? from : to));
  return (
    <div
      className="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 max-w-[min(90vw,420px)] px-4 py-2.5 rounded-[6px] border border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)] text-small font-medium shadow-[var(--shadow)] break-words"
      role="status"
    >
      {text}
    </div>
  );
}
