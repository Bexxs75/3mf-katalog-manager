import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import { useModalDialog } from '../hooks/useModalDialog';
import { toAppError, type AppError } from '../lib/errors';
import { diskSyncReport, onDiskSyncRequest, type DiskReport, type DiskSyncPreview } from '../lib/api/diskSync';
import { applyDiskSync, previewDiskSync, type DiskSyncOutcome } from '../lib/fileOps';
import { fileOpsWait } from '../lib/fileOpsErrors';
import { useCanManageFiles } from '../lib/webSession';
import { ErrorText } from '../diagnostics/ErrorText';
import { Icon } from './Icon';
import { DiskReportPreview, SkippedRows } from './DiskReportPreview';
import { DiskReportResult } from './DiskReportResult';
import { DiskReportRun } from './DiskReportRun';
import { DiskReportSelect } from './DiskReportSelect';
import { DiskReportUnclear } from './DiskReportUnclear';
import { asDiskFailure, classifyDiskFailure, diskSelectionCount, diskSyncSelection, groupMissingFolders, initialDiskSelection, type DiskFailure, type DiskSection, type DiskSelection, type DiskStep } from './DiskReportModel';

const button = 'px-3 py-2 rounded border border-[var(--line)] bg-[var(--panel)] text-body cursor-pointer disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-[var(--accent)]';
const primary = `${button} !border-[var(--accent)] !bg-[var(--accent)] !text-[var(--accent-ink)]`;

function Rows({ items }: { items: { key: string; root: string | null; path: string; note?: string }[] }) {
  return <ul className="space-y-1">{items.map(item => (
    <li key={item.key} className="rounded bg-[var(--panel-2)] p-2 text-small">
      <span className="block break-all font-code">{item.path}</span>
      {item.root && <span className="block text-[var(--ink-3)] break-all">{item.root}</span>}
      {item.note && <span className="block text-[var(--ink-2)]">{item.note}</span>}
    </li>
  ))}</ul>;
}

/** The display-only list: used when the server has no file actions switched on. */
function ReadOnlyReport({ report }: { report: DiskReport }) {
  const t = useT();
  const count = useFormatCount();
  const rootName = (index: number) => report.roots[index]?.name ?? '';
  // With one folder the root name adds nothing; with several it tells equal relative paths apart.
  const showRoot = report.roots.length > 1;
  return <>
    {report.missingModels.length > 0 && <section><h3 className="ui-label mb-2">{t('diskReportMissingHeading')}</h3>
      <p className="mb-1 text-small">{count(t('diskReportMissing'), report.missingModels.length)}</p>
      <Rows items={report.missingModels.map(m => ({ key: m.id, root: showRoot ? rootName(m.root) : null, path: m.relativePath }))} /></section>}
    {report.newFiles.length > 0 && <section><h3 className="ui-label mb-2">{t('diskReportNewHeading')}</h3>
      <p className="mb-1 text-small">{count(t('diskReportNew'), report.newFiles.length)}</p>
      <Rows items={report.newFiles.map(f => ({ key: `${f.root}:${f.relativePath}`, root: showRoot ? rootName(f.root) : null, path: f.relativePath }))} /></section>}
    {report.danglingCompanions.length > 0 && <section><h3 className="ui-label mb-2">{t('diskReportCompanionHeading')}</h3>
      <p className="mb-1 text-small">{count(t('diskReportCompanions'), report.danglingCompanions.length)}</p>
      <Rows items={report.danglingCompanions.map(c => ({ key: c.id, root: showRoot ? rootName(c.root) : null, path: c.relativePath, note: c.reason === 'notFile' ? t('diskReportNotFile') : undefined }))} /></section>}
    <DiskReportUnclear rows={(report.unclear ?? []).map((u, i) => ({ key: `${i}`, path: showRoot && u.root !== undefined ? `${rootName(u.root)}: ${u.relativePath}` : u.relativePath, reason: u.reason }))} />
  </>;
}

function FailureBody({ failure, report }: { failure: DiskFailure; report: DiskReport | null }) {
  const t = useT();
  const text = (() => {
    switch (failure.kind) {
      case 'expired': return t('diskApplyExpired');
      case 'changed': return t('diskApplyStateChanged');
      case 'limit': return failure.retryAfter ? `${t('diskApplyLimit')} ${t('diskApplyRetryIn').replace('{wait}', fileOpsWait(failure.retryAfter))}` : t('diskApplyLimit');
      case 'maintenanceStart': return t('diskApplyMaintenanceStart');
      case 'maintenanceRun': return t('diskApplyMaintenanceRun');
      case 'backupFailed': return t('diskApplyBackupFailed');
      case 'interrupted': return `${t('diskApplyInterrupted')} ${failure.message}`;
      default: return failure.message;
    }
  })();
  const outcome = failure.outcome;
  return <div role="alert" className="space-y-2 rounded border border-[var(--crit)] p-3 text-[var(--ink)]">
    <p className="font-semibold">{text}</p>
    {outcome && <p className="text-small">{t('diskApplyAppliedSoFar').replace('{done}', String(outcome.done)).replace('{total}', String(outcome.total))} {t('diskApplyResult').replace('{added}', String(outcome.imported)).replace('{removed}', String(outcome.removedModels)).replace('{companions}', String(outcome.removedCompanions))}</p>}
    {outcome && outcome.skipped.length > 0 && report && <SkippedRows report={report} rows={outcome.skipped} />}
  </div>;
}

interface DialogProps {
  step: DiskStep; report: DiskReport | null; selection: DiskSelection; preview: DiskSyncPreview | null; progress: { done: number; total: number };
  outcome: DiskSyncOutcome | null; failure: DiskFailure | null; scanError: AppError | null; canApply: boolean;
  onToggle: (section: DiskSection, key: string) => void; onAll: (section: DiskSection) => void; onNone: (section: DiskSection) => void; onFolder: (ids: string[], on: boolean) => void;
  onClose: () => void; onRescan: () => void; onNext: () => void; onBack: () => void; onApply: () => void; onCancel: () => void;
}

function ReportDialog(p: DialogProps) {
  const t = useT();
  const count = useFormatCount();
  const { step, report, preview, selection } = p;
  const running = step === 'starting' || step === 'running' || step === 'cancelling';
  const ref = useModalDialog({ open: true, onClose: p.onClose, busy: running });
  const titleRef = useRef<HTMLHeadingElement>(null);
  const resultRef = useRef<HTMLHeadingElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const previous = useRef<DiskStep | null>(null);
  // Every step change moves the focus to something that still exists: the result heading, the cancel button, or the new title.
  useEffect(() => {
    const before = previous.current;
    previous.current = step;
    if (before === null || before === step) return;
    if (step === 'done' || step === 'cancelled') resultRef.current?.focus();
    else if (step === 'starting') cancelRef.current?.focus();
    else if (!running) titleRef.current?.focus();
  }, [step, running]);

  const total = preview ? preview.counts.import + preview.counts.removeModels + preview.counts.removeCompanions : 0;
  const selected = diskSelectionCount(selection);
  const title = step === 'preview' || step === 'previewing' ? t('diskApplyPreviewTitle')
    : running ? t('diskApplyRunTitle')
    : step === 'done' || step === 'cancelled' ? t('diskApplyDoneTitle')
    : step === 'error' && p.failure ? t('diskApplyErrorTitle') : t('diskReportTitle');
  const outside = report ? report.modelsOutsideRoots + report.companionsOutsideRoots : 0;
  const skippedRoots = report?.roots.filter(r => r.status !== 'ok') ?? [];
  const partial = report?.roots.filter(r => r.mostlyMissing) ?? [];
  const found = !!report && report.missingModels.length + report.newFiles.length + report.danglingCompanions.length > 0;
  const complete = !!report && !report.truncated && skippedRoots.length === 0 && outside === 0;
  const truncatedText = (by: DiskReport['truncatedBy'][number]) => t(by === 'items' ? 'diskReportTruncatedItems' : by === 'entries' ? 'diskReportTruncatedEntries' : 'diskReportTruncatedTime');
  const failureKind = p.failure?.kind;
  const rescanLabel = failureKind === 'scan' ? t('diskSyncRetry') : t('diskApplyRecheck');
  const listing = step === 'select' || step === 'scanning' || (step === 'error' && failureKind === 'scan');
  return createPortal(<div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4">
    <div ref={ref} role="dialog" aria-modal="true" aria-label={title} data-step={step} tabIndex={-1} className="w-[560px] max-w-full max-h-[85vh] flex flex-col rounded-lg bg-[var(--panel)] border border-[var(--line)] shadow-[var(--shadow)]">
      <h2 ref={titleRef} tabIndex={-1} className="text-title font-semibold px-4 py-3 border-b border-[var(--line)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]">{title}</h2>
      <div className="overflow-auto p-4 space-y-4 text-body" aria-busy={step === 'scanning' || step === 'previewing'}>
        {!p.canApply && <p className="text-small text-[var(--ink-2)]">{t('diskReportReadOnly')} {t('diskApplyReadOnlyHint')}</p>}
        {(step === 'scanning' || step === 'previewing') && <p role="status">{t('diskReportRunning')}</p>}
        {step === 'error' && failureKind === 'scan' && p.scanError && <div role="alert" className="text-[var(--crit)] whitespace-pre-wrap break-words">{t('diskReportFailed')} <ErrorText error={p.scanError} /></div>}
        {step === 'error' && p.failure && failureKind !== 'scan' && <FailureBody failure={p.failure} report={report} />}
        {listing && report && <>
          {report.truncatedBy.map(by => <p key={by} role="status" className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{truncatedText(by)}</p>)}
          {skippedRoots.map(r => <p key={r.name + r.status} className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{t(r.status === 'empty' ? 'diskReportRootEmpty' : 'diskReportRootUnreachable').replace('{name}', r.name)}</p>)}
          {partial.map(r => <p key={r.name + 'partial'} role="status" className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{t('diskReportRootMostlyMissing').replace('{name}', r.name)}</p>)}
          {outside > 0 && <p className="rounded bg-[var(--panel-2)] px-2.5 py-1.5 text-small text-[var(--ink-2)]">{count(t('diskReportOutside'), outside)}</p>}
          {!found && complete && <p role="status">{t('diskReportAllGood')}</p>}
          {p.canApply
            ? <DiskReportSelect report={report} selection={selection} onToggle={p.onToggle} onAll={p.onAll} onNone={p.onNone} onFolder={p.onFolder} />
            : <ReadOnlyReport report={report} />}
          {p.canApply && found && <p className="text-small text-[var(--ink-2)]">{t('diskApplyUnclearNote')}</p>}
        </>}
        {step === 'preview' && report && preview && <DiskReportPreview report={report} preview={preview} />}
        {running && <DiskReportRun done={p.progress.done} total={p.progress.total} starting={step === 'starting'} cancelling={step === 'cancelling'} />}
        {(step === 'done' || step === 'cancelled') && report && p.outcome && <DiskReportResult ref={resultRef} report={report} outcome={p.outcome} cancelled={step === 'cancelled'} />}
      </div>
      <div className="flex justify-end gap-2 px-4 py-3 border-t border-[var(--line)]">
        {step === 'error' && failureKind !== 'restart' && <button type="button" className={button} onClick={p.onRescan}>{rescanLabel}</button>}
        {step === 'select' && p.canApply && <>
          <button type="button" className={button} onClick={p.onClose}>{t('startSyncLater')}</button>
          <button type="button" className={primary} disabled={selected === 0} aria-describedby={selected === 0 ? 'disk-apply-blocked' : undefined} onClick={p.onNext}>{selected > 0 ? count(t('diskApplyNext'), selected) : t('diskApplyNextPlain')}</button>
        </>}
        {step === 'preview' && <>
          <button type="button" className={button} onClick={p.onBack}>{t('diskApplyBack')}</button>
          <button type="button" className={primary} disabled={total === 0} aria-describedby={total === 0 ? 'disk-apply-blocked' : undefined} onClick={p.onApply}>{t('diskSyncApply')}</button>
        </>}
        {running && <button type="button" ref={cancelRef} className={button} aria-disabled={step === 'cancelling'} onClick={p.onCancel}>{t('diskApplyCancel')}</button>}
        {(step === 'done' || step === 'cancelled') && <button type="button" className={button} onClick={p.onRescan}>{t('diskApplyRecheck')}</button>}
        {!running && !(step === 'select' && p.canApply) && <button type="button" className={step === 'done' || step === 'cancelled' ? primary : button} onClick={p.onClose}>{t('diskSyncClose')}</button>}
      </div>
      {((step === 'select' && selected === 0) || (step === 'preview' && total === 0)) && p.canApply && <p id="disk-apply-blocked" className="sr-only">{t('diskApplyBlockedHint')}</p>}
    </div>
  </div>, document.body);
}

const emptySelection: DiskSelection = { new: new Set(), missing: new Set(), companions: new Set() };

/** Web: compares the catalog with the disk and, with file actions switched on, applies the differences the user picks. */
export function CatalogDiskReport({ active = true }: { active?: boolean }) {
  const t = useT();
  const canApply = useCanManageFiles();
  const [open, setOpen] = useState(false);
  const [step, setStep] = useState<DiskStep>('scanning');
  const [report, setReport] = useState<DiskReport | null>(null);
  const [selection, setSelection] = useState<DiskSelection>(emptySelection);
  const [preview, setPreview] = useState<DiskSyncPreview | null>(null);
  const [progress, setProgress] = useState({ done: 0, total: 0 });
  const [outcome, setOutcome] = useState<DiskSyncOutcome | null>(null);
  const [failure, setFailure] = useState<DiskFailure | null>(null);
  const [scanError, setScanError] = useState<AppError | null>(null);
  const run = useRef(0);
  const stop = useRef(false);
  const openRef = useRef(false);
  openRef.current = open;

  const scan = useCallback(async () => {
    const mine = ++run.current;
    setOpen(true); setStep('scanning'); setReport(null); setPreview(null); setOutcome(null); setFailure(null); setScanError(null);
    try {
      const result = await diskSyncReport();
      if (run.current !== mine) return;
      setReport(result); setSelection(initialDiskSelection(result)); setStep('select');
    } catch (e) {
      if (run.current !== mine) return;
      setScanError(toAppError(e)); setFailure({ kind: 'scan', message: '', outcome: null }); setStep('error');
    }
  }, []);
  // The notice after the server's start check opens this dialog through the same event the desktop uses.
  useEffect(() => onDiskSyncRequest(() => { if (!openRef.current && !document.querySelector('[role="dialog"], [aria-modal="true"]')) void scan(); }), [scan]);

  const close = () => { run.current++; stop.current = true; setOpen(false); setReport(null); setPreview(null); setOutcome(null); setFailure(null); setScanError(null); };
  const fail = (reason: unknown) => { setFailure(classifyDiskFailure(asDiskFailure(reason))); setStep('error'); };
  const toggle = (section: DiskSection, key: string) => setSelection(current => {
    const next = new Set(current[section]);
    if (next.has(key)) next.delete(key); else next.add(key);
    return { ...current, [section]: next };
  });
  const setAll = (section: DiskSection, all: boolean) => setSelection(current => {
    if (!report) return current;
    const keys = section === 'new' ? report.newFiles.map(f => `${f.root}:${f.relativePath}`) : section === 'missing' ? groupMissingFolders(report).single.map(m => m.id) : report.danglingCompanions.filter(c => c.reason === 'missing').map(c => c.id);
    // "All" / "None" of the plain rows leaves the ticks of the folder rows alone.
    const kept = section === 'missing' ? [...current.missing].filter(id => !keys.includes(id)) : [];
    return { ...current, [section]: new Set(all ? [...kept, ...keys] : kept) };
  });
  const setFolder = (ids: string[], on: boolean) => setSelection(current => {
    const next = new Set(current.missing);
    ids.forEach(id => on ? next.add(id) : next.delete(id));
    return { ...current, missing: next };
  });
  const next = async () => {
    if (!report) return;
    const mine = ++run.current;
    setStep('previewing');
    try {
      const result = await previewDiskSync(diskSyncSelection(report, selection));
      if (run.current === mine) { setPreview(result); setStep('preview'); }
    } catch (reason) { if (run.current === mine) fail(reason); }
  };
  const apply = async () => {
    if (!preview || !preview.token) return;
    const mine = ++run.current;
    stop.current = false;
    setProgress({ done: 0, total: 0 }); setStep('starting');
    try {
      const result = await applyDiskSync(preview.token, preview.removes, (done, total) => {
        if (run.current !== mine) return;
        setProgress({ done, total });
        setStep(current => current === 'starting' ? 'running' : current);
      }, () => stop.current);
      if (run.current !== mine) return;
      setOutcome(result);
      if (result.aborted === 'user') setStep('cancelled');
      else if (result.aborted === 'maintenance') { setFailure({ kind: 'maintenanceRun', message: '', outcome: result }); setStep('error'); }
      else if (result.aborted === 'session') { setFailure({ kind: 'other', message: t('diskApplyInterrupted'), outcome: result }); setStep('error'); }
      else setStep('done');
    } catch (reason) { if (run.current === mine) fail(reason); }
  };
  const cancel = () => { stop.current = true; setStep(current => current === 'starting' || current === 'running' ? 'cancelling' : current); };

  return <>
    {active && <button type="button" className={`${button} shrink-0 flex gap-2 items-center !py-1.5`} disabled={open} title={t('diskReportOpen')} onClick={() => void scan()}>
      <Icon name="restore" size={16} /><span className="max-[1279px]:sr-only">{t('diskReportOpen')}</span>
    </button>}
    {open && <ReportDialog step={step} report={report} selection={selection} preview={preview} progress={progress} outcome={outcome} failure={failure} scanError={scanError} canApply={canApply}
      onToggle={toggle} onAll={s => setAll(s, true)} onNone={s => setAll(s, false)} onFolder={setFolder} onClose={close} onRescan={() => void scan()} onNext={() => void next()} onBack={() => setStep('select')} onApply={() => void apply()} onCancel={cancel} />}
  </>;
}
