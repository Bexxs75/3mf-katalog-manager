import { useEffect, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatCount } from '../i18n/types';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { listPrinterHistory } from '../lib/api/printers';
import type { FilamentSpool, Printer, PrinterDetails, PrinterHistoryJob, PrinterKind, PrinterNavigation } from '../types';
import type { PrintersState } from '../hooks/usePrinters';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import { usePrinterReorder } from '../hooks/usePrinterReorder';
import { PrinterConnectionSection } from './PrinterConnectionSection';
import { PrinterLinkControl } from './PrinterLinkControl';
import { PrinterUnitsSection } from './PrinterUnitsSection';
import { PrinterManagerDialog, pmButton, pmField } from './PrinterManagerDialog';
import { SegmentedControl } from './SegmentedControl';
import { Icon } from './Icon';

interface Props {
  printers: PrintersState;
  printerLink: PrinterLinkState;
  printerId?: string;
  onMaterial: (context: PrinterNavigation) => void;
}

function Card({ n, title, aside, children, busy }: {
  n?: number; title: string; aside?: ReactNode; children: ReactNode; busy?: boolean;
}) {
  return <section aria-busy={busy} className="min-w-0 rounded-lg border border-[var(--line)] bg-[var(--panel)] overflow-hidden">
    <h2 className="px-3.5 py-2.5 text-[13px] font-bold flex items-center gap-2 border-b border-[var(--line)] bg-[var(--panel-2)]">
      {n != null && <><span className="font-mono-ui text-[var(--ink-3)] text-[11px]">{n}</span>{' '}</>}{title}
      {aside != null && <span aria-hidden="true" className="ml-auto font-mono-ui text-[var(--ink-3)] text-[11px] font-normal">{aside}</span>}
    </h2>
    <div className="p-3.5 flex flex-col gap-2.5">{children}</div>
  </section>;
}

export function PrinterManagerView({ printers: state, printerLink, printerId, onMaterial }: Props) {
  const t = useT();
  const [selected, setSelected] = useState<string | undefined>(printerId);
  const [adding, setAdding] = useState(false);
  useEffect(() => { setSelected(printerId); }, [printerId]);
  const printer = state.printers.find(p => p.id === selected) ?? state.printers[0];
  const order = usePrinterReorder(state.printers.map(p => p.id), state.reorderPrinters);
  return <main aria-label={t('railPrinters')} className="flex-1 min-w-0 min-h-0 overflow-auto text-[13px]">
    <div className="max-w-[1900px] w-full mx-auto">
    <div className="p-4 flex flex-wrap items-start gap-4">
      <h1 className="font-bold text-[16px]">Printer Manager</h1>
      <div className="ml-auto max-w-xl"><PrinterLinkControl link={printerLink} printers={state.printers} /></div>
      <button className={pmButton} onClick={() => setAdding(true)}>{t('pmAddPrinter')}</button>
    </div>
    {state.error && <div role="alert" className="p-3 text-[var(--crit)]"><ErrorText error={state.error} /></div>}
    {!printer ? <div className="p-4"><section className="rounded-lg border border-[var(--line)] bg-[var(--panel)] flex flex-col items-center gap-4 px-3.5 py-10 text-center">
      <Icon name="printer" size={24} /><h2 className="font-bold">{t('pmEmptyTitle')}</h2>
      <p className="max-w-lg text-[var(--ink-3)]">{t('pmEmptyBody')}</p>
      <button className={pmButton} onClick={() => setAdding(true)}>{t('pmAddFirst')}</button>
    </section></div> : <div className="grid grid-cols-1 min-[1000px]:grid-cols-[280px_minmax(0,1fr)] min-[1400px]:grid-cols-[280px_minmax(0,1fr)_minmax(0,1fr)] gap-4 p-4 items-start">
      <Card title={t('pmPrinterCard')} aside={state.printers.length}>
        {state.printers.map(p => {
          const conn = printerLink.connections.find(c => c.printerId === p.id);
          const status = !printerLink.enabled || !conn ? 'pmNotConnected' : conn.lastError || conn.paused ? 'pmConnectionError' : 'pmConnected';
          const dot = status === 'pmNotConnected' ? 'var(--ink-3)' : status === 'pmConnectionError' ? 'var(--crit)' : 'var(--good)';
          return <div key={p.id} onMouseEnter={() => order.enter(p.id)} className={`rounded border ${printer.id === p.id ? 'border-[var(--accent)] bg-[var(--accent-soft)]' : order.over === p.id ? 'border-[var(--accent)] bg-[var(--panel)]' : 'border-[var(--line)] bg-[var(--panel)]'}`}>
            <div className="flex items-center">
              <button className="px-1 py-3 cursor-grab focus-visible:outline-2" aria-label={`${t('printersReorderHint')} ${p.name}`}
                onMouseDown={e => { if (e.button === 0) order.begin(e, p.id); }} onKeyDown={e => order.key(e, p.id)}>⋮⋮</button>
              <button aria-current={printer.id === p.id ? 'true' : undefined} className="text-left flex-1 min-w-0 p-2 focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
                onClick={() => setSelected(p.id)}>
                <span className="flex gap-2 items-center font-bold"><span aria-hidden className="w-2 h-2 rounded-full shrink-0" style={{ background: dot }} />{p.name}</span>{' '}
                <span className="block text-[11px] text-[var(--ink-3)]">{p.kind === 'resin' ? t('spoolKindResin') : t('spoolKindFilament')} · {formatCount(t('pmUnitCount'), p.units.length)} · {formatCount(t('printersUnitSlotCount'), p.units.reduce((n, u) => n + u.slotCount, 0))} · {t(status)}</span>
              </button>
            </div>
          </div>;
        })}
        <p className="text-[12px] text-[var(--ink-3)]">{t('pmReorderHint')}</p>
      </Card>
      <PrinterDetail key={printer.id} printer={printer} state={state} link={printerLink} onMaterial={onMaterial} />
    </div>}
    {adding && <AddPrinter state={state} onClose={() => setAdding(false)} onAdded={setSelected} />}
    </div>
  </main>;
}

function AddPrinter({ state, onClose, onAdded }: { state: PrintersState; onClose: () => void; onAdded: (id: string) => void }) {
  const t = useT();
  const [name, setName] = useState('');
  const [kind, setKind] = useState<PrinterKind>('filament');
  return <PrinterManagerDialog title={t('pmAddPrinter')} onClose={onClose} submitLabel={t('printersAddPrinterButton')}
    onSubmit={async () => {
      const p = await state.addPrinter(name.trim(), t(kind === 'resin' ? 'printersKindResinVat' : 'printersKindExternal'), kind);
      onAdded(p.id);
    }}>
    <label>{t('pmName')}<input required maxLength={60} pattern=".*\S.*" className={pmField} value={name} onChange={e => setName(e.target.value)} /></label>
    <SegmentedControl label={t('pmKind')} value={kind} onChange={setKind} options={[
      { value: 'filament', label: t('spoolKindFilament') }, { value: 'resin', label: t('spoolKindResin') },
    ]} />
  </PrinterManagerDialog>;
}

function PrinterDetail({ printer, state, link, onMaterial }: {
  printer: Printer; state: PrintersState; link: PrinterLinkState; onMaterial: Props['onMaterial'];
}) {
  const t = useT();
  const { language } = useLanguage();
  const [spools, setSpools] = useState<FilamentSpool[]>([]);
  const [jobs, setJobs] = useState<PrinterHistoryJob[]>([]);
  const [spoolsReady, setSpoolsReady] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [revision, setRevision] = useState(0);
  const [deleting, setDeleting] = useState(false);
  useEffect(() => {
    let active = true;
    setError(null);
    Promise.all([invoke<FilamentSpool[]>('list_filament_spools'), listPrinterHistory(printer.id)])
      .then(([s, j]) => { if (active) { setSpools(s); setJobs(j); setSpoolsReady(true); } })
      .catch(e => { if (active) setError(toAppError(e)); });
    return () => { active = false; };
  }, [printer.id, revision, link.jobs]);
  const count = spools.filter(s => printer.units.some(u => u.id === s.unitId)).length;
  const number = new Intl.NumberFormat(language, { maximumFractionDigits: 1 });
  return <div className="min-w-0 flex flex-col gap-4 min-[1400px]:col-span-2 min-[1400px]:grid min-[1400px]:grid-cols-2 items-start">
    {error && <div role="alert" className="p-3 text-[var(--crit)] min-[1400px]:col-span-2"><ErrorText error={error} /></div>}
    <div className="w-full min-w-0 flex flex-col gap-4">
    <Card n={1} title={`${t('pmPrinterCard')}: ${printer.name}`}><General printer={printer} state={state}
      deleteAction={<button type="button" className={`${pmButton} ml-auto text-[var(--crit)]`} disabled={!spoolsReady} onClick={() => setDeleting(true)}>{t('pmDeletePrinter')}</button>} /></Card>
    <Card n={2} title={t('pmConnection')}>
      {printer.kind === 'resin' ? <p>{t('pmResinNoConnection')}</p> : !link.enabled ? <p>{t('pmLinkOff')}</p> : <>
        <PrinterConnectionSection printerId={printer.id} link={link} connection={link.connections.find(c => c.printerId === printer.id) ?? null} />
      </>}
      <p className="text-[var(--ink-3)]">{t('pmFutureConnections')}</p>
    </Card>
    </div>
    <div className="w-full min-w-0 flex flex-col gap-4">
    <Card n={3} title={t('pmUnits')} busy={!spoolsReady} aside={formatCount(t('printersUnitSlotCount'), printer.units.reduce((n, u) => n + u.slotCount, 0))}>
      {spoolsReady && <PrinterUnitsSection printer={printer} spools={spools} actions={state}
        onChanged={() => setRevision(n => n + 1)} onMaterial={() => onMaterial({ printerId: printer.id })} />}
    </Card>
    <Card n={4} title={t('pmJobs')} busy={!spoolsReady}>
      {!spoolsReady ? null : jobs.length === 0 ? <p className="text-[var(--ink-3)]">{t('pmNoJobs')}</p> : <div className="overflow-auto"><table className="w-full text-left text-[12px]">
        <thead className="font-mono-ui text-[10.5px] uppercase tracking-[.08em] text-[var(--ink-3)]"><tr>{(['pmFile', 'pmOutcome', 'pmDuration', 'pmUsage', 'pmStatus'] as const).map(k => <th className="p-2 border-b border-[var(--line)]" key={k}>{t(k)}</th>)}</tr></thead>
        <tbody>{jobs.map(j => {
          const grams = j.grams ?? link.jobs.find(open => open.id === j.id)?.grams;
          return <tr key={j.id}>
          <td className="p-2">{j.fileName}</td><td className="p-2">{t(j.outcome === 'completed' ? 'printerJobCompleted' : 'printerJobPartial')}</td>
          <td className="p-2 whitespace-nowrap">{t('printerJobsMinutes').replace('{min}', number.format(j.printDurationS / 60))}</td>
          <td className="p-2 whitespace-nowrap">{grams != null ? `${number.format(grams)} g` : `${number.format(j.usedMm)} mm`}</td>
          <td className="p-2">{j.state === 'open' ? <button className={`${pmButton} h-6 !py-0`} onClick={() => onMaterial({ printerId: printer.id, reviewJobs: true })}>{t('pmConfirmJobs')}</button> : t(j.state === 'confirmed' ? 'pmBooked' : 'pmIgnored')}</td>
        </tr>; })}</tbody>
      </table></div>}
    </Card>
    </div>
    {deleting && <PrinterManagerDialog title={t('printersDeletePrinterConfirm').replace('{name}', printer.name)} onClose={() => setDeleting(false)}
      submitLabel={t('pmDeletePrinter')} onSubmit={async () => { await state.deletePrinter(printer.id); await link.refresh(); }}>
      {count > 0 && <p>{formatCount(t('printersDeleteReturnHomeCount'), count)}</p>}
    </PrinterManagerDialog>}
  </div>;
}

function General({ printer, state, deleteAction }: { printer: Printer; state: PrintersState; deleteAction: ReactNode }) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [name, setName] = useState(printer.name);
  const initial = () => ({ manufacturer: printer.manufacturer ?? '', model: printer.model ?? '', nozzleMm: String(printer.nozzleMm ?? ''), bedXMm: String(printer.bedXMm ?? ''), bedYMm: String(printer.bedYMm ?? ''), bedZMm: String(printer.bedZMm ?? '') });
  const [draft, setDraft] = useState(initial);
  const [validation, setValidation] = useState('');
  const [error, setError] = useState<AppError | null>(null);
  const reset = () => { setName(printer.name); setDraft(initial()); setValidation(''); setError(null); };
  const displayed = editing ? draft : initial();
  const field = (key: keyof typeof draft, label: string) => <label>{label}<input className={pmField} value={displayed[key]} readOnly={!editing} onChange={e => setDraft({ ...draft, [key]: e.target.value })} /></label>;
  return <form onSubmit={async e => {
    e.preventDefault();
    if (!editing || busy) return;
    const numeric = (s: string) => s.trim() === '' ? null : Number(s.replace(',', '.'));
    const details: PrinterDetails = { manufacturer: draft.manufacturer.trim() || null, model: draft.model.trim() || null,
      nozzleMm: numeric(draft.nozzleMm), bedXMm: numeric(draft.bedXMm), bedYMm: numeric(draft.bedYMm), bedZMm: numeric(draft.bedZMm) };
    const invalid = (n: number | null, min: number, max: number) => n !== null && (!Number.isFinite(n) || n < min || n > max);
    if (invalid(details.nozzleMm, 0.1, 2)) { setValidation(t('pmInvalidNozzle')); return; }
    if ([details.bedXMm, details.bedYMm, details.bedZMm].some(n => invalid(n, 1, 2000))) { setValidation(t('pmInvalidBed')); return; }
    setBusy(true); setValidation(''); setError(null);
    try { await state.updateDetails(printer.id, details); if (name.trim() !== printer.name) await state.renamePrinter(printer.id, name.trim()); setEditing(false); }
    catch (e) { setError(toAppError(e)); } finally { setBusy(false); }
  }}>
    <fieldset disabled={busy} className="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <label>{t('pmName')}<input required maxLength={60} pattern=".*\S.*" className={pmField} value={editing ? name : printer.name} readOnly={!editing} onChange={e => setName(e.target.value)} /></label>
      <div>{t('pmKind')}<div className="py-2">{t(printer.kind === 'resin' ? 'spoolKindResin' : 'spoolKindFilament')} {t('pmFixed')}</div></div>
      {field('manufacturer', t('pmManufacturer'))}{field('model', t('pmModel'))}
      {printer.kind === 'filament' && field('nozzleMm', t('pmNozzle'))}
      <fieldset><legend>{t('pmBed')}</legend><div className="grid grid-cols-3 gap-2">{(['bedXMm', 'bedYMm', 'bedZMm'] as const).map((key, i) => <input key={key} aria-label={`${t('pmBed')} ${'XYZ'[i]}`} className={pmField} value={displayed[key]} readOnly={!editing} onChange={e => setDraft({ ...draft, [key]: e.target.value })} />)}</div></fieldset>
    </fieldset>
    <p className="text-[var(--ink-3)] mt-3">{t('pmBedHint')}</p>
    {validation && <p role="alert" className="text-[var(--crit)] mt-2">{validation}</p>}
    {error && <div role="alert"><ErrorText error={error} /></div>}
    <div className="flex flex-wrap gap-2 mt-3">{editing ? <>
      <button className={pmButton} disabled={busy}>{t('printersSave')}</button>
      <button type="button" className={pmButton} disabled={busy} onClick={() => { reset(); setEditing(false); }}>{t('printersCancel')}</button>
    </> : <button type="button" className={pmButton} onClick={() => { reset(); setEditing(true); }}>{t('pmEdit')}</button>}{deleteAction}</div>
  </form>;
}
