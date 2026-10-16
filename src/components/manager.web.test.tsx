import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { login, authenticationFailed } from '../lib/webSession';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { FilamentSpoolForm } from './FilamentSpoolForm';
import { usePrinters } from '../hooks/usePrinters';
import type { FilamentSpool } from '../types';

const spool: FilamentSpool = { id: '1', kind: 'filament', material: 'PLA', manufacturer: null, color: null, colorHex: null, location: 'Shelf', homeLocation: null, unitId: null, slotIndex: null, diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 500, price: null, imagePng: null };
let calls: {command: string; args: any}[] = [];
let writable = true;
beforeEach(async () => {
  vi.stubEnv('VITE_TARGET', 'web'); localStorage.setItem('3mf-katalog-language', 'de'); calls = []; writable = true;
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({authenticated: true, writable, csrfToken: 't'});
    const command = url.split('/').pop()!; calls.push({command, args: JSON.parse(String(options?.body))});
    return Response.json(command === 'list_printers' ? [] : null);
  }));
  vi.stubGlobal('IntersectionObserver', class { observe() {} disconnect() {} });
  await login('password');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); vi.unstubAllEnvs(); });
function form(editing = spool) { return render(<LanguageProvider><FilamentSpoolForm open editing={editing} knownLocations={[]} onSaved={() => {}} onClose={() => {}} /></LanguageProvider>); }
it('reads printers in web read-only mode', async () => {
  writable = false; await login('password'); renderHook(() => usePrinters());
  await waitFor(() => expect(calls.map(c => c.command)).toContain('list_printers'));
});
it('keeps an unloaded existing image by omitting imagePng', async () => {
  form({...spool, hasImage: true} as FilamentSpool);
  fireEvent.click(screen.getByRole('button', {name: 'Speichern'}));
  await waitFor(() => expect(calls.some(c => c.command === 'update_filament_spool')).toBe(true));
  expect(calls.find(c => c.command === 'update_filament_spool')!.args.spool).not.toHaveProperty('imagePng');
});
it('offers a browser image input and explicit image removal', async () => {
  form({...spool, imagePng: 'old'});
  expect(document.querySelector('input[type=file]')).toHaveAttribute('accept', 'image/*');
  fireEvent.click(screen.getByRole('button', {name: 'Bild entfernen'}));
  fireEvent.click(screen.getByRole('button', {name: 'Speichern'}));
  await waitFor(() => expect(calls.find(c => c.command === 'update_filament_spool')?.args.spool.imagePng).toBeNull());
});
it('disables a form when write permission is revoked', async () => {
  form(); await act(async () => authenticationFailed(401));
  expect(screen.getByRole('button', {name: 'Speichern'})).toBeDisabled();
  fireEvent.click(screen.getByRole('button', {name: 'Speichern'}));
  expect(calls).toEqual([]);
});

import { within } from '@testing-library/react';
import { FilamentView } from './FilamentView';
import { PrinterManagerView } from './PrinterManagerView';
import { usePrinterLink } from '../hooks/usePrinterLink';
import { WEB_READ_COMMANDS } from '../lib/readOnly';
import { invoke } from '../lib/transport';
import { MetadataError } from './MetadataError';
import { clearMetadataError } from '../lib/metadataErrors';
import type { Printer, PrinterHistoryJob } from '../types';
import { de } from '../i18n/de';
const printer: Printer = { id: 'p1', name: 'Printer', kind: 'filament', manufacturer: null, model: null, nozzleMm: null, bedXMm: null, bedYMm: null, bedZMm: null, units: [
  { id: 'u1', printerId: 'p1', name: 'Custom', kind: 'custom', slotCount: 4, bambuAmsIndex: null },
  { id: 'u2', printerId: 'p1', name: 'Holder', kind: 'external', slotCount: 1, bambuAmsIndex: null },
] };
const history: PrinterHistoryJob = { id: 'job', fileName: 'saved.gcode', outcome: 'completed', printDurationS: 60, usedMm: 10, grams: 1, state: 'open', endedAt: 123 } as PrinterHistoryJob;
const writes = () => calls.filter(c => !(WEB_READ_COMMANDS as readonly string[]).includes(c.command));
function mockManager(extra?: (command: string, args: any) => Response | undefined) {
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({authenticated: true, writable, csrfToken: 't'});
    const command = url.split('/').pop()!; const args = JSON.parse(String(options?.body)); calls.push({command, args});
    const override = extra?.(command, args); if (override) return override;
    const result: Record<string, unknown> = {list_printers: [printer, {...printer, id: 'p2', name: 'Other', units: []}], list_filament_spools: [spool, {...spool, id: 'loaded', material: 'PETG', unitId: 'u1', slotIndex: 0, homeLocation: 'Shelf'}], list_printer_history: [history], load_spool: {displacedSpoolId: 'loaded'}, unload_spool: 'Shelf', add_printer: printer};
    return Response.json(result[command] ?? null);
  }));
}
function managers(view: 'material' | 'printers') {
  function Wrapper() {
    const printers = usePrinters(); const printerLink = usePrinterLink();
    return <><MetadataError />{view === 'material' ? <FilamentView printers={printers} printerLink={printerLink} /> : <PrinterManagerView printers={printers} printerLink={printerLink} onMaterial={() => {}} />}</>;
  }
  return render(<LanguageProvider><Wrapper /></LanguageProvider>);
}
it.each([false, true])('all nine printer/unit mutations dispatch exactly their command when writable=%s', async enabled => {
  writable = enabled; await login('password'); mockManager();
  const {result} = renderHook(() => usePrinters());
  await waitFor(() => expect(result.current.printers.length).toBe(2)); calls = [];
  const actions = [
    ['add_printer', () => result.current.addPrinter('New', 'Holder', 'filament')],
    ['rename_printer', () => result.current.renamePrinter('p1', 'Renamed')],
    ['update_printer_details', () => result.current.updateDetails('p1', {manufacturer: 'Maker', model: null, nozzleMm: null, bedXMm: null, bedYMm: null, bedZMm: null})],
    ['reorder_printers', () => result.current.reorderPrinters(['p2','p1'])],
    ['delete_printer', () => result.current.deletePrinter('p1')],
    ['add_unit', () => result.current.addUnit('p1', 'custom', 'New', 2)],
    ['update_unit', () => result.current.updateUnit('u1', 'Updated', 2)],
    ['delete_unit', () => result.current.deleteUnit('u1')],
    ['reorder_units', () => result.current.reorderUnits('p1', ['u2','u1'])],
  ] as const;
  for (const [command, action] of actions) {
    calls = []; await act(async () => { await action().catch(() => {}); });
    expect(writes().map(c => c.command)).toEqual(enabled ? [command] : []);
    if (enabled) expect(calls[calls.length - 1]?.command).toBe('list_printers');
  }
});
it.each([false,true])('material controls and slot actions respect writable=%s, networking and restock stay hidden', async enabled => {
  writable = enabled; await login('password'); mockManager(); managers('material');
  const slot = await screen.findByTestId('slot-u1-0');
  await screen.findByTestId('spool-card-1');
  expect(screen.queryByRole('button', {name: de.filamentRestockButton})).toBeNull();
  expect(screen.queryByRole('button', {name: de.resinConsumeButton})).toBeNull();
  expect(screen.getByRole('button', {name: de.filamentOpenAddPanelButton})).toHaveProperty('disabled', !enabled);
  calls = []; fireEvent.click(slot);
  if (!enabled) {
    expect(slot).toBeDisabled(); expect(screen.queryByRole('menu')).toBeNull();
    fireEvent.doubleClick(screen.getByTestId('spool-card-1'));
    fireEvent.mouseDown(screen.getByTestId('spool-card-1'), {button:0,clientX:0,clientY:0});
    fireEvent.mouseMove(document, {clientX:30,clientY:30}); fireEvent.mouseEnter(slot); fireEvent.mouseUp(document);
    expect(screen.queryByRole('dialog')).toBeNull(); expect(writes()).toEqual([]); return;
  }
  fireEvent.click(within(screen.getByRole('menu')).getByRole('button', {name: /PLA/}));
  await waitFor(() => expect(writes().map(c=>c.command)).toEqual(['load_spool']));
  expect(writes()[0].args).toEqual({spoolId:'1',unitId:'u1',slotIndex:0});
  await screen.findByText('PETG wurde aus dem belegten Slot an den Stammplatz zurückgelegt.');
  calls = []; fireEvent.click(slot);
  fireEvent.click(screen.getByRole('button', {name: de.printersSlotMenuUnload}));
  await waitFor(() => expect(writes().map(c=>c.command)).toEqual(['unload_spool']));
  calls = []; fireEvent.click(screen.getByRole('button', {name: de.deleteAriaLabel}));
  expect(screen.getByText(de.webManagerDeleteSpool)).toBeInTheDocument(); expect(writes()).toEqual([]);
  fireEvent.click(screen.getByRole('button', {name: de.delete}));
  await waitFor(() => expect(writes().map(c=>c.command)).toEqual(['delete_filament_spool']));
});
it.each([false,true])('printer view keeps saved history and hides printer networking with writable=%s', async enabled => {
  writable = enabled; await login('password'); mockManager(); managers('printers');
  await screen.findByText('saved.gcode');
  expect(screen.queryByRole('switch')).toBeNull();
  expect(screen.queryByRole('button',{name:de.pmConfirmJobs})).toBeNull();
  expect(screen.queryByRole('button',{name:de.printerSyncNow})).toBeNull();
  expect(screen.queryByRole('heading',{name:de.pmConnection})).toBeNull();
  expect(screen.getByRole('button',{name:de.pmDeletePrinter})).toHaveProperty('disabled',!enabled);
  expect(screen.getByRole('button',{name:de.pmAddPrinter})).toHaveProperty('disabled',!enabled);
  if (enabled) {
    fireEvent.click(screen.getByRole('button',{name:de.pmDeletePrinter}));
    expect(screen.getByText(de.webManagerDeletePrinter)).toBeInTheDocument();
    expect(writes()).toEqual([]);
    fireEvent.submit(screen.getByRole('dialog').querySelector('form')!);
    await waitFor(() => expect(writes().map(c=>c.command)).toEqual(['delete_printer']));
  } else {
    fireEvent.keyDown(screen.getByRole('button',{name:`${de.printersReorderHint} Printer`}),{key:'ArrowDown'});
    expect(writes()).toEqual([]);
  }
});
it('shows a slot shrink warning and unit deletion consequences before mutation', async () => {
  mockManager(); managers('printers'); await screen.findByText('saved.gcode');
  const units = screen.getByRole('heading',{name:de.pmUnits}).closest('section')!;
  fireEvent.click(within(units).getAllByRole('button',{name:de.pmEdit})[0]);
  fireEvent.change(screen.getByRole('spinbutton'),{target:{value:'2'}});
  expect(screen.getByText(de.webManagerShrinkUnit)).toBeInTheDocument(); expect(writes()).toEqual([]);
  fireEvent.submit(screen.getByRole('dialog').querySelector('form')!);
  await waitFor(() => expect(writes().map(c=>c.command)).toEqual(['update_unit']));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull()); calls=[];
  fireEvent.click(screen.getByRole('button',{name:`${de.printersDelete} Custom`}));
  expect(screen.getByText(de.webManagerDeleteUnit)).toBeInTheDocument(); expect(writes()).toEqual([]);
  fireEvent.submit(screen.getByRole('dialog').querySelector('form')!);
  await waitFor(() => expect(writes().map(c=>c.command)).toEqual(['delete_unit']));
});
it('shows no connection status in the web printer list, because printers cannot be linked there', async () => {
  mockManager(); managers('printers'); await screen.findByText('saved.gcode');
  expect(screen.queryByText(de.pmInactive)).toBeNull();
  expect(screen.queryByText(de.pmNoConnection)).toBeNull();
  expect(screen.getAllByText(de.spoolKindFilament).length).toBeGreaterThan(0);
});
it('keeps successful copies after a partial create and retries only missing copies', async () => {
  let adds=0;
  mockManager(command=> command==='add_filament_spool' && ++adds === 2 ? Response.json({message:'quota',expected:true},{status:422}) : undefined);
  render(<LanguageProvider><FilamentSpoolForm open editing={null} knownLocations={[]} onSaved={()=>{}} onClose={()=>{}} /></LanguageProvider>);
  fireEvent.change(screen.getByPlaceholderText(de.filamentMaterialLabel), {target:{value:'PLA'}});
  fireEvent.change(screen.getByRole('spinbutton'), {target:{value:'3'}});
  fireEvent.click(screen.getByRole('button',{name:/Hinzufügen/}));
  await screen.findByText('1 angelegt. Noch 2 fehlen; erneut Hinzufügen legt nur diese an.');
  expect(screen.getByRole('spinbutton')).toHaveValue(2);
  calls=[]; fireEvent.click(screen.getByRole('button',{name:/Hinzufügen/}));
  await waitFor(()=>expect(writes().map(c=>c.command)).toEqual(['add_filament_spool','add_filament_spool']));
});
it('sends the selected validated browser image as base64', async () => {
  form();
  const file = new File([''], 'image.png', {type:'image/png'});
  Object.defineProperty(file,'arrayBuffer',{value: async()=>new Uint8Array([137,80,78,71,13,10,26,10]).buffer});
  fireEvent.change(document.querySelector('input[type=file]')!,{target:{files:[file]}});
  await screen.findByRole('button',{name:de.webManagerRemoveImage});
  fireEvent.click(screen.getByRole('button',{name:de.filamentSaveButton}));
  await waitFor(()=>expect(writes()[0]?.args.spool.imagePng).toBe('iVBORw0KGgo='));
});
it.each([429,503])('retries a manager mutation once on %s with Retry-After', async status => {
  let attempt=0;
  mockManager(command=>command==='rename_printer' && ++attempt===1 ? Response.json({message:'busy'},{status,headers:{'Retry-After':'0'}}):undefined);
  const {result}=renderHook(()=>usePrinters());
  await act(async()=>{await result.current.renamePrinter('p1','New');});
  expect(writes().map(c=>c.command)).toEqual(['rename_printer','rename_printer']);
});
it('shows a failed reorder and a failed reload without optimistic ordering', async () => {
  let failRead=false; clearMetadataError();
  mockManager(command=> command==='reorder_printers' || (failRead && command==='list_printers') ? Response.json({message:'offline'},{status:500}):undefined);
  managers('printers'); await screen.findByText('saved.gcode');
  fireEvent.keyDown(screen.getByRole('button',{name:`${de.printersReorderHint} Printer`}),{key:'ArrowDown'});
  await screen.findAllByText('offline');
  expect(calls.find(c=>c.command==='reorder_printers')?.args).toEqual({ids:['p2','p1']});
  cleanup(); clearMetadataError(); calls=[]; failRead=true;
  managers('printers'); await screen.findAllByText('offline');
});
it.each(['restock_filament_spool','consume_resin','set_printer_link_enabled','test_printer_connection','remove_printer_connection','sync_printers_now','preview_printer_job','get_printer_job_thumbnail','confirm_printer_jobs','ignore_printer_job','pick_and_read_image'])('never sends forbidden %s even in write mode', async command => {
  await expect(invoke(command,{})).rejects.toBeDefined(); expect(calls).toEqual([]);
});
it('waits for image validation before saving and never silently drops a selected image', async () => {
  form(); let finish!: (buffer:ArrayBuffer)=>void;
  const file=new File([''], 'image.png',{type:'image/png'});
  Object.defineProperty(file,'arrayBuffer',{value:()=>new Promise<ArrayBuffer>(resolve=>{finish=resolve;})});
  fireEvent.change(document.querySelector('input[type=file]')!,{target:{files:[file]}});
  const save=screen.getByRole('button',{name:de.filamentSaveButton});
  expect(save).toBeDisabled(); fireEvent.click(save); expect(writes()).toEqual([]);
  await act(async()=>{finish(new Uint8Array([137,80,78,71,13,10,26,10]).buffer);});
  expect(save).not.toBeDisabled(); fireEvent.click(save);
  await waitFor(()=>expect(writes()[0]?.args.spool.imagePng).toBe('iVBORw0KGgo='));
});
it.each(['large','format'])('rejects a %s image visibly without sending an update',async kind=>{
  form(); const file=new File(['invalid'],'image.png',{type:'image/png'});
  Object.defineProperty(file,'arrayBuffer',{value:async()=>new Uint8Array([1,2,3]).buffer});
  if(kind==='large') Object.defineProperty(file,'size',{value:5*1024*1024+1});
  fireEvent.change(document.querySelector('input[type=file]')!,{target:{files:[file]}});
  await screen.findByText(kind==='large'?de.webPhotoTooLarge:de.webPhotoFormat);
  fireEvent.click(screen.getByRole('button',{name:de.filamentSaveButton})); expect(writes()).toEqual([]);
});
it('does not send a spool form mutation in read-only mode, even through its save handler', async()=>{
  writable=false; await login('password'); form();
  fireEvent.click(screen.getByRole('button',{name:de.filamentSaveButton}));
  expect(writes()).toEqual([]);
});
it('reorders printers from the UI and waits for the refreshed server order',async()=>{
  mockManager(); managers('printers'); await screen.findByText('saved.gcode'); calls=[];
  fireEvent.keyDown(screen.getByRole('button',{name:`${de.printersReorderHint} Printer`}),{key:'ArrowDown'});
  await waitFor(()=>expect(calls.map(c=>c.command)).toEqual(['reorder_printers','list_printers']));
});
it('retains a visible reload error after a successful printer write',async()=>{
  let changed=false; clearMetadataError();
  mockManager(command=>{
    if(command==='rename_printer') changed=true;
    if(command==='list_printers'&&changed) return Response.json({message:'reload failure',expected:true},{status:500});
  });
  const {result}=renderHook(()=>usePrinters()); render(<LanguageProvider><MetadataError /></LanguageProvider>);
  await waitFor(()=>expect(result.current.printers).toHaveLength(2));
  await act(async()=>{await result.current.renamePrinter('p1','New');});
  expect(screen.getByRole('alert')).toHaveTextContent('reload failure');
});
it('retries busy manager reads once in read-only mode',async()=>{
  writable=false; await login('password'); let attempts=0;
  mockManager(command=>command==='list_printers' && ++attempts===1 ? Response.json({message:'busy'},{status:503,headers:{'Retry-After':'0'}}):undefined);
  const {result}=renderHook(()=>usePrinters()); await waitFor(()=>expect(result.current.printers).toHaveLength(2));
  expect(calls.map(c=>c.command)).toEqual(['list_printers','list_printers']);
});
