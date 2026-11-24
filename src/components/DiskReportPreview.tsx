import { useFormatCount, useT } from '../i18n/LanguageContext';
import type { DiskReport, DiskSyncPreview } from '../lib/api/diskSync';
import { diskRowLabel, skipReasonKey } from './DiskReportModel';

/** Lists rows the server did not apply or took out of the plan, each with its reason. */
export function SkippedRows({ report, rows }: { report: DiskReport; rows: { kind: 'import' | 'removeModel' | 'removeCompanion'; key: string; reason: string }[] }) {
  const t = useT();
  return <ul className="space-y-1">{rows.map(row => <li key={`${row.kind}:${row.key}`} className="rounded bg-[var(--panel-2)] p-2 text-small">
    <span className="block break-all font-code">{diskRowLabel(report, row.kind, row.key)}</span>
    <span className="block text-[var(--ink-2)]">{t(skipReasonKey(row.reason))}</span>
  </li>)}</ul>;
}

/** What applying will do, as the server's plan states it (rows already left out are listed too). */
export function DiskReportPreview({ report, preview }: { report: DiskReport; preview: DiskSyncPreview }) {
  const t = useT();
  const count = useFormatCount();
  const { counts } = preview;
  return <>
    <p>{t('diskApplyPreviewIntro')}</p>
    <ul className="space-y-2">
      {counts.import > 0 && <li><b className="block">{count(t('diskApplySummaryImport'), counts.import)}</b><span className="block text-small text-[var(--ink-2)]">{t('diskApplyImportNote')}</span></li>}
      {counts.removeModels > 0 && <li><b className="block">{count(t('diskApplySummaryRemove'), counts.removeModels)}</b><span className="block text-small text-[var(--ink-2)]">{t('diskApplyRemoveTrashNote')}</span></li>}
      {counts.removeCompanions > 0 && <li><b className="block">{count(t('diskApplySummaryCompanions'), counts.removeCompanions)}</b><span className="block text-small text-[var(--ink-2)]">{t('diskApplyCompanionNote')}</span></li>}
    </ul>
    {preview.mostlyMissingRoots.map(name => <p key={name} role="status" className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{t('diskReportRootMostlyMissing').replace('{name}', name)}</p>)}
    {preview.skipped.length > 0 && <section aria-labelledby="disk-preview-skipped">
      <h3 id="disk-preview-skipped" className="ui-label mb-1">{count(t('diskApplySkippedHeading'), preview.skipped.length)}</h3>
      <SkippedRows report={report} rows={preview.skipped} />
    </section>}
    <p className="text-small text-[var(--ink-2)]">{t('diskApplyRecheckNote')}</p>
    {preview.removes && <p className="text-small text-[var(--ink-2)]">{t('diskApplyBackupNote')}</p>}
  </>;
}
