import { useEffect } from 'react';
import { useT } from '../i18n/LanguageContext';
import type { DragOutHint } from '../hooks/useDragOutTransition';

export function DragOutToast({ hint, onDone, onSettings }: {
  hint: DragOutHint; onDone: () => void; onSettings: () => void;
}) {
  const t = useT();
  useEffect(() => {
    const timer = setTimeout(onDone, 15000);
    return () => clearTimeout(timer);
  }, [hint, onDone]);
  return <div role="status" className="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 max-w-[90vw] rounded border border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)] px-4 py-3 shadow-[var(--shadow)]">
    {hint === 'dgOutsideOffHint'
      ? <button type="button" className="text-left underline focus-visible:outline focus-visible:outline-2" onClick={() => { onDone(); onSettings(); }}>{t(hint)}</button>
      : t(hint)}
  </div>;
}
