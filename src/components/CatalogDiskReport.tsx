import { useCallback, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import { useModalDialog } from '../hooks/useModalDialog';
import { toAppError, type AppError } from '../lib/errors';
import { diskSyncReport, type DiskReport } from '../lib/api/diskSync';
import { ErrorText } from '../diagnostics/ErrorText';
import { Icon } from './Icon';

const button = 'px-3 py-2 rounded border border-[var(--line)] bg-[var(--panel)] text-body cursor-pointer disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

function Rows({ items }: { items: { key: string; root: string | null; path: string; note?: string }[] }) {
  return <ul className="space-y-1">{items.map(item => (
    <li key={item.key} className="rounded bg-[var(--panel-2)] p-2 text-small">
      <span className="block break-all font-code">{item.path}</span>
      {item.root && <span className="block text-[var(--ink-3)] break-all">{item.root}</span>}
      {item.note && <span className="block text-[var(--ink-2)]">{item.note}</span>}
    </li>
  ))}</ul>;
}

function ReportDialog({ report, error, busy, onClose, onRetry }: {
  report: DiskReport | null; error: AppError | null; busy: boolean; onClose: () => void; onRetry: () => void;
}) {
  const t = useT();
  const count = useFormatCount();
  const ref = useModalDialog({ open: true, onClose, busy });
  const rootName = (index: number) => report?.roots[index]?.name ?? '';
  // With one folder the root name adds nothing; with several it tells equal relative paths apart.
  const showRoot = (report?.roots.length ?? 0) > 1;
  const outside = report ? report.modelsOutsideRoots + report.companionsOutsideRoots : 0;
  const skipped = report?.roots.filter(r => r.status !== 'ok') ?? [];
  const found = !!report && (report.missingModels.length + report.newFiles.length + report.danglingCompanions.length > 0);
  const complete = !!report && !report.truncated && skipped.length === 0 && outside === 0;
  const truncatedText = (by: DiskReport['truncatedBy'][number]) => t(by === 'items' ? 'diskReportTruncatedItems' : by === 'entries' ? 'diskReportTruncatedEntries' : 'diskReportTruncatedTime');
  return createPortal(<div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4">
    <div ref={ref} role="dialog" aria-modal="true" aria-label={t('diskReportTitle')} tabIndex={-1} className="w-[560px] max-w-full max-h-[85vh] flex flex-col rounded-lg bg-[var(--panel)] border border-[var(--line)] shadow-[var(--shadow)]">
      <h2 className="text-title font-semibold px-4 py-3 border-b border-[var(--line)]">{t('diskReportTitle')}</h2>
      <div className="overflow-auto p-4 space-y-4 text-body">
        <p className="text-small text-[var(--ink-2)]">{t('diskReportReadOnly')}</p>
        {busy && <p role="status">{t('diskReportRunning')}</p>}
        {error && <div role="alert" className="text-[var(--crit)] whitespace-pre-wrap break-words">{t('diskReportFailed')} <ErrorText error={error} /></div>}
        {report && <>
          {report.truncatedBy.map(by => <p key={by} role="status" className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{truncatedText(by)}</p>)}
          {skipped.map(r => <p key={r.name + r.status} className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{t(r.status === 'empty' ? 'diskReportRootEmpty' : 'diskReportRootUnreachable').replace('{name}', r.name)}</p>)}
          {outside > 0 && <p className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{count(t('diskReportOutside'), outside)}</p>}
          {!found && complete && <p role="status">{t('diskReportAllGood')}</p>}
          {report.missingModels.length > 0 && <section><h3 className="ui-label mb-2">{t('diskReportMissingHeading')}</h3>
            <p className="mb-1 text-small">{count(t('diskReportMissing'), report.missingModels.length)}</p>
            <Rows items={report.missingModels.map(m => ({ key: m.id, root: showRoot ? rootName(m.root) : null, path: m.relativePath }))} /></section>}
          {report.newFiles.length > 0 && <section><h3 className="ui-label mb-2">{t('diskReportNewHeading')}</h3>
            <p className="mb-1 text-small">{count(t('diskReportNew'), report.newFiles.length)}</p>
            <Rows items={report.newFiles.map(f => ({ key: `${f.root}:${f.relativePath}`, root: showRoot ? rootName(f.root) : null, path: f.relativePath }))} /></section>}
          {report.danglingCompanions.length > 0 && <section><h3 className="ui-label mb-2">{t('diskReportCompanionHeading')}</h3>
            <p className="mb-1 text-small">{count(t('diskReportCompanions'), report.danglingCompanions.length)}</p>
            <Rows items={report.danglingCompanions.map(c => ({ key: c.id, root: showRoot ? rootName(c.root) : null, path: c.relativePath, note: c.reason === 'notFile' ? t('diskReportNotFile') : undefined }))} /></section>}
        </>}
      </div>
      <div className="flex justify-end gap-2 px-4 py-3 border-t border-[var(--line)]">
        {error && <button type="button" className={button} disabled={busy} onClick={onRetry}>{t('diskSyncRetry')}</button>}
        <button type="button" className={button} onClick={onClose}>{t('diskSyncClose')}</button>
      </div>
    </div>
  </div>, document.body);
}

/** Web only: compares the catalog with the disk and lists the differences. Reads only; nothing here changes a file or the catalog. */
export function CatalogDiskReport({ active = true }: { active?: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<DiskReport | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const run = useRef(0);
  const start = useCallback(async () => {
    const mine = ++run.current;
    setOpen(true); setBusy(true); setReport(null); setError(null);
    try {
      const result = await diskSyncReport();
      if (run.current === mine) setReport(result);
    } catch (e) {
      if (run.current === mine) setError(toAppError(e));
    } finally {
      if (run.current === mine) setBusy(false);
    }
  }, []);
  const close = () => { run.current++; setOpen(false); setBusy(false); setReport(null); setError(null); };
  return <>
    {active && <button type="button" className={`${button} shrink-0 flex gap-2 items-center !py-1.5`} disabled={busy} title={t('diskReportOpen')} onClick={() => void start()}>
      <Icon name="restore" size={16} /><span className="max-[1099px]:sr-only">{t('diskReportOpen')}</span>
    </button>}
    {open && <ReportDialog report={report} error={error} busy={busy} onClose={close} onRetry={() => void start()} />}
  </>;
}
