import { useMemo, useState } from 'react';
import { displayName } from '../lib/displayName';
import { useCanEditMetadata } from '../lib/webSession';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { setPrinterJobFailure } from '../lib/api/printerLink';
import { printStatusGroup } from '../lib/printStatusGroups';
import type { PrinterHistoryJob, PrinterNavigation } from '../types';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import { Icon } from './Icon';
import { PrinterManagerDialog, pmButton } from './PrinterManagerDialog';
import { PrintFailureReasonField, useFailureReasonText } from './PrintFailureReasonField';

/** Rows drawn at first and per click on "Show more"; the history can hold thousands. */
export const HISTORY_PAGE_SIZE = 100;

/** "Result" of one print: headline, colour and the finer group behind a failure. */
function useResultText() {
  const t = useT();
  return (job: PrinterHistoryJob) => {
    const group = printStatusGroup(job.rawStatus, job.failureReason);
    switch (group) {
      case 'succeeded': return { text: t('pfResultSucceeded'), sub: '', tone: 'good' as const };
      case 'cancelled': return { text: t('pfResultCancelled'), sub: '', tone: 'warn' as const };
      case 'failed': return { text: t('pfResultFailed'), sub: '', tone: 'crit' as const };
      case 'printer_error': return { text: t('pfResultFailed'), sub: t('pfGroupPrinterError'), tone: 'crit' as const };
      case 'interrupted': return { text: t('pfResultFailed'), sub: t('pfGroupInterrupted'), tone: 'crit' as const };
      default: return { text: t('pfResultFailed'), sub: t('pfGroupOther').replace('{status}', () => job.rawStatus || '?'), tone: 'crit' as const };
    }
  };
}

const TONE = {
  good: 'bg-[var(--good-soft)] text-[var(--good)]',
  warn: 'bg-[var(--warn-soft)] text-[var(--warn)]',
  crit: 'bg-[var(--crit-soft)] text-[var(--crit)]',
};

function ReasonDialog({ job, onClose, onSave }: { job: PrinterHistoryJob; onClose: () => void; onSave: (reason: string | null, note: string | null) => Promise<void> }) {
  const t = useT();
  const [reason, setReason] = useState<string | null>(job.failureReason);
  const [note, setNote] = useState(job.failureNote ?? '');
  return <PrinterManagerDialog title={t('pfEditReasonFor').replace('{file}', () => displayName(job.fileName))} onClose={onClose}
    submitLabel={t('printersSave')} onSubmit={() => onSave(reason, reason === 'other' && note.trim() ? note : null)}>
    <PrintFailureReasonField reason={reason} note={note} onChange={(r, n) => { setReason(r); setNote(n); }} />
    <p className="text-[var(--ink-3)]">{t('pfEditHint')}</p>
  </PrinterManagerDialog>;
}

/** The "Recent prints" table of a printer: result, reason, and the way to set the reason afterwards. */
export function PrinterHistoryTable({ printerId, jobs, link, onMaterial, onChanged }: {
  printerId: string; jobs: PrinterHistoryJob[]; link: PrinterLinkState; onMaterial: (context: PrinterNavigation) => void; onChanged: () => void;
}) {
  const t = useT();
  const isWeb = import.meta.env.VITE_TARGET === 'web';
  const { language } = useLanguage();
  const canEdit = useCanEditMetadata();
  const reasonText = useFailureReasonText();
  const resultOf = useResultText();
  const [shown, setShown] = useState(HISTORY_PAGE_SIZE);
  const [editing, setEditing] = useState<PrinterHistoryJob | null>(null);
  const number = useMemo(() => new Intl.NumberFormat(language, { maximumFractionDigits: 1 }), [language]);
  const visible = jobs.slice(0, shown);
  const hasAutoClosed = jobs.some(j => j.autoClosed);
  const canSetReason = canEdit && !isWeb;
  const columns = ['pmFile', 'pmDuration', 'pmUsage', 'pmResult', 'pmReason', 'pmStatus'] as const;
  return <>
    {/* Scrolls on its own so a long history does not push the page down; the head stays in view. */}
    <div role="region" aria-label={t('pmJobs')} tabIndex={0} className="overflow-auto max-h-[28rem]"><table className="w-full text-left text-small">
      <thead className="ui-label text-[var(--ink-3)] sticky top-0 bg-[var(--panel)]"><tr>{columns.map(k => <th className="p-2 border-b border-[var(--line)]" key={k}>{t(k)}</th>)}</tr></thead>
      <tbody>{visible.map(j => {
        const grams = j.grams ?? link.jobs.find(open => open.id === j.id)?.grams;
        const result = resultOf(j);
        const raw = t('pfPrinterReports').replace('{status}', () => j.rawStatus || '?');
        return <tr key={j.id}>
          <td className="p-2">{displayName(j.fileName)}</td>
          <td className="p-2 whitespace-nowrap">{t('printerJobsMinutes').replace('{min}', number.format(j.printDurationS / 60))}</td>
          <td className="p-2 whitespace-nowrap">{grams != null ? `${number.format(grams)} g` : j.usedMm > 0 ? `${number.format(j.usedMm)} mm` : '–'}</td>
          <td className="p-2" title={raw}>
            <span className={`text-caption font-semibold px-1.5 py-0.5 rounded ${TONE[result.tone]}`}>{result.text}</span>
            {result.sub && <span className="text-caption text-[var(--ink-2)]"> · {result.sub}</span>}
            <span className="sr-only"> {raw}</span>
          </td>
          <td className="p-2"><div className="flex items-center gap-1.5">
            <span className="flex-1 min-w-0">{j.failureReason
              ? <>{reasonText(j.failureReason)}{j.failureNote && <span className="block text-caption text-[var(--ink-3)] break-words">{j.failureNote}</span>}</>
              : <span className="text-[var(--ink-3)]">–</span>}</span>
            {canSetReason && j.outcome === 'partial' &&
              <button type="button" className={`${pmButton} !p-1 !border-transparent hover:bg-[var(--panel-2)]`} title={t('pfEditReason')}
                aria-label={t('pfEditReasonFor').replace('{file}', () => displayName(j.fileName))} onClick={() => setEditing(j)}><Icon name="edit" size={14} /></button>}
          </div></td>
          <td className="p-2">{j.autoClosed
            ? <span className="text-caption px-1.5 py-0.5 rounded bg-[var(--panel-2)] text-[var(--ink-2)] whitespace-nowrap" title={raw}>{t(j.usedMm > 0 ? 'pmOlderHistory' : 'pmNoUsage')}</span>
            : j.state === 'open' ? isWeb ? t('webManagerJobOpen') : <button className={`${pmButton} h-6 !py-0`} onClick={() => onMaterial({ printerId, reviewJobs: true })}>{t('pmConfirmJobs')}</button>
            : t(j.state === 'confirmed' ? 'pmBooked' : 'pmIgnored')}</td>
        </tr>;
      })}</tbody>
    </table></div>
    {jobs.length > shown && <button type="button" className={`${pmButton} self-start`} onClick={() => setShown(n => n + HISTORY_PAGE_SIZE)}>
      {t('pmShowMore').replace('{count}', () => String(jobs.length - shown))}</button>}
    {hasAutoClosed && <p className="text-caption text-[var(--ink-3)]">{t('pmAutoClosedNote')}</p>}
    {isWeb && <p className="text-caption text-[var(--ink-3)]">{t('pfWebReadOnly')}</p>}
    {editing && <ReasonDialog job={editing} onClose={() => setEditing(null)}
      onSave={async (reason, note) => { await setPrinterJobFailure(editing.id, reason, note); onChanged(); }} />}
  </>;
}
