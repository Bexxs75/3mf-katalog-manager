import { useT } from '../i18n/LanguageContext';

/** Progress of the blocks. The bar and the text are two views of the same count; only the text is announced. */
export function DiskReportRun({ done, total, starting, cancelling }: { done: number; total: number; starting: boolean; cancelling: boolean }) {
  const t = useT();
  const percent = total > 0 ? Math.round(done / total * 100) : 0;
  return <>
    <p role="status">{starting ? t('diskApplyStarting') : cancelling ? t('diskApplyCancelling') : t('diskApplyRunning').replace('{done}', String(done)).replace('{total}', String(total))}</p>
    <div role="progressbar" aria-label={t('diskApplyProgressLabel')} aria-valuemin={0} aria-valuemax={total} aria-valuenow={done} className="h-2 rounded bg-[var(--panel-2)] overflow-hidden">
      <div className="h-full bg-[var(--accent)]" style={{ width: `${percent}%` }} />
    </div>
    <p className="text-small text-[var(--ink-2)]">{t('diskApplyRunNote')} {t('diskApplyCancelHint')}</p>
  </>;
}
