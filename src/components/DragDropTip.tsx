import { useEffect, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { dismissDragDropTip, isDragDropTipDismissed, subscribeDragDropTipDismissed } from '../lib/dragDropTip';

export function DragDropTip({ modelCount, folderCount }: { modelCount: number; folderCount: number }) {
  const t = useT();
  const [dismissed, setDismissed] = useState(isDragDropTipDismissed);
  useEffect(() => subscribeDragDropTipDismissed(() => setDismissed(true)), []);
  if (dismissed || modelCount < 1 || folderCount < 1) return null;
  return (
    <aside className="absolute bottom-4 right-4 z-40 max-w-[min(90%,360px)] px-4 py-3 rounded-[6px] border border-[var(--line-strong)] bg-[var(--ink)] text-[var(--bg)] shadow-[var(--shadow)] text-[12.5px]">
      <p className="font-semibold">{t('dndTipTitle')}</p>
      <p className="mt-1 opacity-90">{t('dndTipBody')}</p>
      <button className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] mt-2 px-2 py-1 rounded border border-current cursor-pointer" onClick={dismissDragDropTip}>{t('dndTipDismiss')}</button>
    </aside>
  );
}
