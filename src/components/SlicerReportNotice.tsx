import { useFormatCount, useT } from '../i18n/LanguageContext';
import { useSlicerReports } from '../hooks/SlicerReportsContext';

const small = 'px-2.5 py-1 rounded-[4px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[length:var(--font-size-meta)] font-semibold cursor-pointer hover:bg-[var(--panel-2)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/** Shown when a slicer has reported files the notice has not mentioned yet. Closing brings it back only for newer reports. */
export function SlicerReportNotice() {
  const t = useT();
  const formatCount = useFormatCount();
  const { unseen, open, openInbox, dismissNotice, inboxOpen } = useSlicerReports();
  if (unseen.length === 0 || inboxOpen) return null;
  return (
    <div role="region" aria-label={t('srInboxTitle')} className="pointer-events-auto max-w-[min(90vw,480px)] flex items-start gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-[var(--ink)]">
      <div className="flex min-w-0 flex-col gap-1.5 [overflow-wrap:anywhere]">
        <span>{formatCount(t('srNoticeText'), open.length)}</span>
        <div className="flex gap-2">
          <button type="button" className={small} onClick={openInbox}>{t('srNoticeView')}</button>
          <button type="button" className={small} onClick={dismissNotice}>{t('srNoticeClose')}</button>
        </div>
      </div>
    </div>
  );
}
