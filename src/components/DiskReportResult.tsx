import { forwardRef } from 'react';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import type { DiskReport } from '../lib/api/diskSync';
import type { DiskSyncOutcome } from '../lib/fileOps';
import { SkippedRows } from './DiskReportPreview';

/** Outcome of a run that ended on its own or was stopped; the heading takes the focus so a screen reader starts there. */
export const DiskReportResult = forwardRef<HTMLHeadingElement, { report: DiskReport; outcome: DiskSyncOutcome; cancelled: boolean }>(function DiskReportResult({ report, outcome, cancelled }, headingRef) {
  const t = useT();
  const count = useFormatCount();
  const summary = t('diskApplyResult').replace('{added}', String(outcome.imported)).replace('{removed}', String(outcome.removedModels)).replace('{companions}', String(outcome.removedCompanions));
  return <>
    <h3 ref={headingRef} tabIndex={-1} className="ui-label focus-visible:outline-2 focus-visible:outline-[var(--accent)]">{cancelled ? t('diskApplyCancelled').replace('{done}', String(outcome.done)).replace('{total}', String(outcome.total)) : t('diskApplyDone')}</h3>
    <div aria-live="polite" className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small">{summary}</div>
    {outcome.skipped.length > 0 && <section aria-labelledby="disk-result-skipped">
      <h3 id="disk-result-skipped" className="ui-label mb-1">{count(t('diskApplySkippedHeading'), outcome.skipped.length)}</h3>
      <SkippedRows report={report} rows={outcome.skipped} />
    </section>}
  </>;
});
