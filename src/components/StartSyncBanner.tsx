import { useFormatCount, useT } from '../i18n/LanguageContext';
import type { StartSyncNotice } from '../hooks/useStartSync';

interface Props { notice: StartSyncNotice | null; onView: () => void; onClose: () => void }

const button = 'px-2.5 py-1 rounded border border-[var(--line)] bg-[var(--panel)] cursor-pointer text-small hover:bg-[var(--panel-2)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/** Result of the sync at start; stays until closed so a change is not missed. */
export function StartSyncBanner({ notice, onView, onClose }: Props) {
  const t = useT();
  const count = useFormatCount();
  if (!notice) return null;
  const join = (parts: string[]) => parts.join(', ');
  const sentence = (key: 'startSyncFound' | 'startSyncAdded' | 'startSyncStillOpen', parts: string[]) => parts.length ? t(key).replace('{items}', join(parts)) : '';
  const open = [
    ...(notice.missing > 0 ? [count(t('diskSyncMissing'), notice.missing)] : []),
    ...(notice.duplicates > 0 ? [count(t('startSyncSameContent'), notice.duplicates)] : []),
  ];
  const added = notice.added;
  const text = notice.unreachable > 0 ? count(t('startSyncUnreachable'), notice.unreachable) : (added ? [
    sentence('startSyncAdded', [
      ...(added.files > 0 ? [count(t('diskSyncNewFiles'), added.files)] : []),
      ...(added.renames > 0 ? [count(t('startSyncRenames'), added.renames)] : []),
    ]),
    sentence('startSyncStillOpen', open),
    added.failed > 0 ? `${count(t('startSyncFailed'), added.failed)}.` : '',
  ] : [
    sentence('startSyncFound', [
      ...(notice.newFiles > 0 ? [count(t('diskSyncNewFiles'), notice.newFiles)] : []),
      ...(notice.renamed > 0 ? [count(t('diskSyncRenamed'), notice.renamed)] : []),
      ...open,
    ]),
  ]).filter(Boolean).join(' ');
  // "Later" only fits a notice that still asks for a decision; after an automatic take-over it is just "Close".
  const decision = !added;
  return (
    <div role="status" className="pointer-events-auto max-w-[min(90vw,520px)] flex flex-col gap-2 px-3.5 py-2.5 rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-[var(--ink)]">
      <p className="break-words">{text}</p>
      <div className="flex justify-end gap-2">
        {notice.unreachable === 0 && <button type="button" className={button} onClick={onView}>{t('startSyncView')}</button>}
        <button type="button" className={button} onClick={onClose}>{t(decision && notice.unreachable === 0 ? 'startSyncLater' : 'startSyncClose')}</button>
      </div>
    </div>
  );
}
