import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
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
import { PrinterStatusBadge, printerStatus, printerStatusColors } from './PrinterStatusBadge';

interface Props {
  printers: PrintersState;
  printerLink: PrinterLinkState;
  printerId?: string;
  onMaterial: (context: PrinterNavigation) => void;
}

function Card({ title, aside, children, busy }: {
  title: string; aside?: ReactNode; children: ReactNode; busy?: boolean;
}) {
  return <section aria-busy={busy} className="min-w-0 rounded-lg border border-[var(--line)] bg-[var(--panel)] overflow-hidden">
    <h2 className="px-[18px] pt-3.5 text-[15px] font-bold flex items-center gap-2">
      {title}
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
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: KeyboardEvent) => {
      if (document.querySelector('[role="dialog"][aria-modal="true"]')) return;
      if (event.key === 'Escape' && !event.defaultPrevented) { setOpen(false); menuRef.current?.focus(); }
    };
    document.addEventListener('keydown', close);
    return () => document.removeEventListener('keydown', close);
  }, [open]);
  return <div aria-label={t('railPrinters')} data-list-open={open} className="relative flex-1 min-w-0 min-h-0 grid grid-cols-[var(--pm-master-width)_minmax(0,1fr)] text-[13px]">
    {open && <button aria-label={t('pmToggleList')} className="absolute inset-0 left-[300px] z-20 bg-[var(--scrim)] min-[1024px]:hidden"
      onClick={() => { setOpen(false); menuRef.current?.focus(); }} />}
    <aside aria-label={t('pmPrinters')} data-collapsed={!open}
      className={`group col-start-1 row-start-1 min-h-0 min-w-0 flex flex-col border-r border-[var(--line)] bg-[var(--panel)] ${open ? 'max-[1023px]:absolute max-[1023px]:inset-y-0 max-[1023px]:left-0 max-[1023px]:w-[300px] max-[1023px]:z-30 max-[1023px]:[box-shadow:var(--shadow)]' : ''}`}>
      <div className="flex items-center gap-2 p-3 max-[1023px]:group-data-[collapsed=true]:justify-center">
        <button type="button" ref={menuRef} aria-label={t('pmToggleList')} aria-expanded={open} className={`${pmButton} min-[1024px]:hidden !px-2`} onClick={() => setOpen(!open)}><Icon name={open ? 'collapse-all' : 'expand-all'} /></button>
        <h2 className="flex-1 uppercase font-mono-ui text-[11px] text-[var(--ink-3)] max-[1023px]:group-data-[collapsed=true]:hidden">{t('pmPrinters')}</h2>
        <span className="text-[var(--ink-3)] max-[1023px]:group-data-[collapsed=true]:hidden">{state.printers.length}</span>
        <button type="button" className={`${pmButton} !px-2 max-[1023px]:group-data-[collapsed=true]:hidden`} aria-label={t('pmAddPrinter')} title={t('pmAddPrinter')} onClick={() => setAdding(true)}>+</button>
      </div>
      <ul className="flex-1 min-h-0 overflow-auto px-2 space-y-0.5">
        {state.printers.map((p, index) => {
          const conn = printerLink.connections.find(c => c.printerId === p.id);
          const status = printerStatus(p, printerLink.enabled, conn);
          const active = printer?.id === p.id;
          return <li key={p.id} onMouseEnter={() => order.enter(p.id)}
            className={`group/item rounded-lg border flex items-center ${active ? 'border-[var(--sel-line)] bg-[var(--sel)] hover:bg-[var(--panel-2)]' : order.over === p.id ? 'border-[var(--sel-line)] bg-[var(--panel-2)]' : 'border-transparent hover:bg-[var(--panel-2)]'}`}>
            <div className="flex w-full items-center">
              <button className={`px-1 py-3 cursor-grab text-[var(--ink-3)] focus-visible:outline-2 focus-visible:opacity-100 group-hover/item:opacity-80 max-[1023px]:group-data-[collapsed=true]:hidden ${active ? 'opacity-80' : 'opacity-0'}`}
                aria-label={`${t('printersReorderHint')} ${p.name}`} onMouseDown={e => { if (e.button === 0) order.begin(e, p.id); }} onKeyDown={e => order.key(e, p.id)}>⋮⋮</button>
              <button aria-current={active ? 'true' : undefined} title={`${p.name} · ${t(status)}`} aria-label={`${p.name} ${t(status)}`}
                className="relative text-left flex flex-1 min-w-0 items-center gap-2.5 p-2 focus-visible:outline-2 focus-visible:outline-[var(--accent)] max-[1023px]:group-data-[collapsed=true]:justify-center"
                onKeyDown={e => {
                  if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
                  e.preventDefault();
                  const next = state.printers[index + (e.key === 'ArrowDown' ? 1 : -1)];
                  if (next) {
                    setSelected(next.id);
                    const entries = e.currentTarget.closest('ul')?.querySelectorAll<HTMLButtonElement>('button[title]');
                    entries?.[index + (e.key === 'ArrowDown' ? 1 : -1)]?.focus();
                  }
                }} onClick={() => { setSelected(p.id); setOpen(false); }}>
                <Icon name={p.kind === 'resin' ? 'resin' : 'printer'} size={24} className="shrink-0 text-[var(--ink-2)]" />
                <span className="min-w-0 flex-1 max-[1023px]:group-data-[collapsed=true]:hidden">
                  <b className="block truncate">{p.name}</b>
                  <span className="flex items-center gap-2 mt-1 min-w-0"><PrinterStatusBadge status={status} />
                    <span className="truncate font-mono-ui text-[10.5px] text-[var(--ink-3)]">{p.kind === 'resin' ? t('spoolKindResin') : conn?.address ?? t('pmNoConnection')}</span>
                  </span>
                </span>
                <span aria-hidden="true" className={`hidden max-[1023px]:group-data-[collapsed=true]:block absolute right-1 top-1 rounded-full w-2 h-2 ${printerStatusColors[status]}`}><span className="block w-full h-full rounded-full bg-current" /></span>
              </button>
            </div>
          </li>;
        })}
      </ul>
      <footer className="border-t border-[var(--line)] p-3 max-[1023px]:group-data-[collapsed=true]:px-0"><PrinterLinkControl link={printerLink} /></footer>
    </aside>
    <main className="col-start-2 row-start-1 min-w-0 min-h-0 overflow-auto p-4 min-[1400px]:px-6">
      {state.error && <div role="alert" className="p-3 text-[var(--crit)]"><ErrorText error={state.error} /></div>}
      {!printer ? <section className="rounded-lg border border-[var(--line)] bg-[var(--panel)] flex flex-col items-center gap-4 px-3.5 py-10 text-center">
        <Icon name="printer" size={24} /><h1 className="font-bold">{t('pmEmptyTitle')}</h1>
        <p className="max-w-lg text-[var(--ink-3)]">{t('pmEmptyBody')}</p>
        <button className={pmButton} onClick={() => setAdding(true)}>{t('pmAddFirst')}</button>
      </section> : <PrinterDetail key={printer.id} printer={printer} state={state} link={printerLink} onMaterial={onMaterial} />}
    </main>
    {adding && <AddPrinter state={state} onClose={() => setAdding(false)} onAdded={id => { setSelected(id); setOpen(false); }} />}
  </div>;
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
  const connection = link.connections.find(c => c.printerId === printer.id);
  return <div className="min-w-0 grid grid-cols-1 min-[1320px]:grid-cols-2 gap-4 items-start max-w-[1500px]">
    {error && <div role="alert" className="p-3 text-[var(--crit)] min-[1320px]:col-span-2"><ErrorText error={error} /></div>}
    <General printer={printer} state={state} header={<>
      <div className="w-12 h-12 shrink-0 rounded-lg border border-[var(--line)] bg-[var(--panel)] grid place-items-center"><Icon name={printer.kind === 'resin' ? 'resin' : 'printer'} /></div>
      <div className="flex-1 min-w-0"><h1 className="text-[22px] font-bold break-words">{printer.name}</h1>
        <div className="mt-1.5 flex flex-wrap items-center gap-3 text-[var(--ink-2)]">
          <span>{t(printer.kind === 'resin' ? 'spoolKindResin' : 'spoolKindFilament')}</span>
          {connection && printer.kind !== 'resin' && <><span>{t('printerConnectionTypeMoonraker')}</span><span className="font-mono-ui text-[12px]">{connection.address}</span></>}
          <PrinterStatusBadge status={printerStatus(printer, link.enabled, connection)} />
        </div>
      </div>
    </>} deleteAction={<button type="button" className={`${pmButton} !border-transparent hover:bg-[var(--panel-2)] text-[var(--crit)] max-[639px]:w-full`} disabled={!spoolsReady} onClick={() => setDeleting(true)}>{t('pmDeletePrinter')}</button>} />
    <Card title={t('pmConnection')}>
      {printer.kind === 'resin' ? <div className="rounded-lg border border-[var(--line)] bg-[var(--panel-2)] p-3"><b>{t('pmNotLinked')}</b><p>{t('pmResinNoConnection')}</p></div> :
        <PrinterConnectionSection printerId={printer.id} link={link} connection={connection ?? null} />}
      <p className="text-[var(--ink-3)]">{t('pmFutureConnections')}</p>
    </Card>
    <Card title={t('pmUnits')} busy={!spoolsReady} aside={formatCount(t('printersUnitSlotCount'), printer.units.reduce((n, u) => n + u.slotCount, 0))}>
      {spoolsReady && <PrinterUnitsSection printer={printer} spools={spools} actions={state}
        onChanged={() => setRevision(n => n + 1)} onMaterial={() => onMaterial({ printerId: printer.id })} />}
    </Card>
    {jobs.length > 0 && <Card title={t('pmJobs')}>
      <div className="overflow-auto"><table className="w-full text-left text-[12px]">
        <thead className="font-mono-ui text-[10.5px] uppercase tracking-[.08em] text-[var(--ink-3)]"><tr>{(['pmFile', 'pmDuration', 'pmUsage', 'pmStatus'] as const).map(k => <th className="p-2 border-b border-[var(--line)]" key={k}>{t(k)}</th>)}</tr></thead>
        <tbody>{jobs.map(j => {
          const grams = j.grams ?? link.jobs.find(open => open.id === j.id)?.grams;
          return <tr key={j.id}>
          <td className="p-2" title={t(j.outcome === 'completed' ? 'printerJobCompleted' : 'printerJobPartial')}>{j.fileName}</td>
          <td className="p-2 whitespace-nowrap">{t('printerJobsMinutes').replace('{min}', number.format(j.printDurationS / 60))}</td>
          <td className="p-2 whitespace-nowrap">{grams != null ? `${number.format(grams)} g` : `${number.format(j.usedMm)} mm`}</td>
          <td className="p-2">{j.state === 'open' ? <button className={`${pmButton} h-6 !py-0`} onClick={() => onMaterial({ printerId: printer.id, reviewJobs: true })}>{t('pmConfirmJobs')}</button> : t(j.state === 'confirmed' ? 'pmBooked' : 'pmIgnored')}</td>
        </tr>; })}</tbody>
      </table></div>
    </Card>}
    {deleting && <PrinterManagerDialog title={t('printersDeletePrinterConfirm').replace('{name}', printer.name)} onClose={() => setDeleting(false)}
      submitLabel={t('pmDeletePrinter')} onSubmit={async () => { await state.deletePrinter(printer.id); await link.refresh(); }}>
      {count > 0 && <p>{formatCount(t('printersDeleteReturnHomeCount'), count)}</p>}
    </PrinterManagerDialog>}
  </div>;
}

function General({ printer, state, header, deleteAction }: { printer: Printer; state: PrintersState; header: ReactNode; deleteAction: ReactNode }) {
  const t = useT();
  const formId = useId();
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [name, setName] = useState(printer.name);
  const initial = () => ({ manufacturer: printer.manufacturer ?? '', model: printer.model ?? '', nozzleMm: String(printer.nozzleMm ?? ''), bedXMm: String(printer.bedXMm ?? ''), bedYMm: String(printer.bedYMm ?? ''), bedZMm: String(printer.bedZMm ?? '') });
  const [draft, setDraft] = useState(initial);
  const [validation, setValidation] = useState('');
  const [error, setError] = useState<AppError | null>(null);
  const reset = () => { setName(printer.name); setDraft(initial()); setValidation(''); setError(null); };
  const displayed = editing ? draft : initial();
  const field = (key: keyof typeof draft, label: string) => <label className="min-w-0">{label}<input className={pmField} value={displayed[key]} readOnly={!editing} onChange={e => setDraft({ ...draft, [key]: e.target.value })} /></label>;
  return <>
    <header data-testid="printer-detail-header" className="flex flex-wrap items-center gap-4 min-[1320px]:col-span-2">
      {header}
      <div className="ml-auto flex flex-wrap gap-2 max-[639px]:w-full">{editing ? <>
        <button form={formId} type="submit" className={`${pmButton} max-[639px]:w-full`} disabled={busy}>{t('printersSave')}</button>
        <button type="button" className={`${pmButton} max-[639px]:w-full`} disabled={busy} onClick={() => { reset(); setEditing(false); }}>{t('printersCancel')}</button>
      </> : <><button type="button" className={`${pmButton} max-[639px]:w-full`} onClick={() => { reset(); setEditing(true); }}>{t('pmEdit')}</button>{deleteAction}</>}</div>
    </header>
    <Card title={t('pmHardware')}>
    <form id={formId} onSubmit={async e => {
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
      <div>{t('pmKind')}<div className="mt-1 px-2 py-1.5 rounded border border-[var(--line)] bg-[var(--panel-2)] text-[var(--ink-2)]">{t(printer.kind === 'resin' ? 'spoolKindResin' : 'spoolKindFilament')} {t('pmFixed')}</div></div>
      {field('manufacturer', t('pmManufacturer'))}{field('model', t('pmModel'))}
      <div className="sm:col-span-2 flex flex-wrap items-end gap-5 max-[639px]:flex-col max-[639px]:items-stretch">
        {printer.kind === 'filament' && <label className="min-w-0">{t('pmNozzle')}<div className="flex items-center gap-1.5"><input aria-label={t('pmNozzle')} className={`${pmField} !w-[9ch] max-[639px]:!w-full text-right font-mono-ui`} value={displayed.nozzleMm} readOnly={!editing} onChange={e => setDraft({ ...draft, nozzleMm: e.target.value })} /><span className="text-[var(--ink-3)] font-mono-ui text-[11px]">mm</span></div></label>}
        <fieldset className="min-w-0"><legend>{t('pmBed')}</legend><div className="flex items-center gap-1.5 max-[639px]:flex-col max-[639px]:items-stretch">{(['bedXMm', 'bedYMm', 'bedZMm'] as const).map((key, i) => <span className="flex items-center gap-1.5 min-w-0" key={key}><input aria-label={`${t('pmBed')} ${'XYZ'[i]}`} className={`${pmField} !w-[9ch] max-[639px]:!w-full text-right font-mono-ui`} value={displayed[key]} readOnly={!editing} onChange={e => setDraft({ ...draft, [key]: e.target.value })} />{i < 2 && <span className="text-[var(--ink-3)] max-[639px]:hidden">×</span>}</span>)}<span className="font-mono-ui text-[11px] text-[var(--ink-3)]">mm</span></div></fieldset>
      </div>
    </fieldset>
    <p className="text-[var(--ink-3)] mt-3">{t('pmBedHint')}</p>
    {validation && <p role="alert" className="text-[var(--crit)] mt-2">{validation}</p>}
    {error && <div role="alert"><ErrorText error={error} /></div>}
  </form>
  </Card>
  </>;
}
