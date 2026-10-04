import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
import { usePrinters } from '../hooks/usePrinters';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { FilamentSpool, Printer, PrinterHistoryJob } from '../types';
import { PrinterManagerView } from './PrinterManagerView';
import { suggestUnitName } from './PrinterUnitsSection';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const details = { manufacturer: null, model: null, nozzleMm: null, bedXMm: null, bedYMm: null, bedZMm: null };
const a: Printer = { ...details, id: '1', name: 'Qidi', kind: 'filament', units: [
  { id: 'u1', printerId: '1', name: 'AMS A', kind: 'bambu_ams', slotCount: 4, bambuAmsIndex: 0 },
  { id: 'u2', printerId: '1', name: 'Holder', kind: 'external', slotCount: 1, bambuAmsIndex: null },
] };
const b: Printer = { ...details, id: '2', name: 'Saturn', kind: 'resin', units: [
  { id: 'vat', printerId: '2', name: 'Harzwanne', kind: 'resin_vat', slotCount: 1, bambuAmsIndex: null },
] };
const spool: FilamentSpool = { id: 's1', material: 'PLA', manufacturer: null, color: 'Rot', location: null,
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 600, price: null, imagePng: null,
  colorHex: '#ff0000', homeLocation: 'Regal', unitId: 'u1', slotIndex: 0, kind: 'filament' };
let list: Printer[];
let history: PrinterHistoryJob[];
const link = (): PrinterLinkState => ({ enabled: false, connections: [], jobs: [], error: null,
  refresh: vi.fn().mockResolvedValue(undefined), setEnabled: vi.fn().mockResolvedValue(undefined),
  testConnection: vi.fn(), removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn() });

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  list = [structuredClone(a), structuredClone(b)]; history = [];
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    const p = args as Record<string, unknown>;
    switch (cmd) {
      case 'list_printers': return structuredClone(list);
      case 'list_filament_spools': return [spool];
      case 'list_printer_history': return p.printerId === '1' ? history : [];
      case 'update_printer_details': Object.assign(list.find(x => x.id === p.id)!, p.details); return;
      case 'rename_printer': list.find(x => x.id === p.printerId)!.name = p.name as string; return;
      case 'delete_printer': list = list.filter(x => x.id !== p.printerId); return 1;
      case 'add_printer': { const next = { ...b, id: '3', name: p.name as string }; list.push(next); return next; }
      default: return;
    }
  });
});
function setup(printerId?: string, printerLink = link()) {
  const onMaterial = vi.fn();
  function Wrapper() {
    const printers = usePrinters();
    return <PrinterManagerView printers={printers} printerLink={printerLink} printerId={printerId} onMaterial={onMaterial} />;
  }
  render(<LanguageProvider><Wrapper /></LanguageProvider>);
  return { onMaterial, printerLink };
}
const general = () => screen.getByRole('heading', { name: '1 Allgemein' }).closest('section')!;
const units = () => screen.getByRole('heading', { name: '2 Materialeinheiten' }).closest('section')!;

it('shows the empty state and creates a resin printer through the keyboard form', async () => {
  list = []; setup();
  fireEvent.click(await screen.findByRole('button', { name: '+ Ersten Drucker hinzufügen' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog).toHaveAttribute('aria-modal', 'true');
  fireEvent.change(within(dialog).getByLabelText('Name'), { target: { value: 'Resin new' } });
  fireEvent.click(within(dialog).getByRole('button', { name: 'Resin' }));
  fireEvent.submit(dialog.querySelector('form')!);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('add_printer', { name: 'Resin new', holderName: 'Harzwanne', kind: 'resin' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(await screen.findByText('Für Resin-Drucker gibt es noch keine Druckeranbindung.')).toBeInTheDocument();
});

it('shows list, all four sections and read-only slots; selects the other printer', async () => {
  setup();
  await screen.findByText('1 · PLA · Rot');
  for (const title of ['1 Allgemein', '2 Materialeinheiten', '3 Verbindung', '4 Druckaufträge']) expect(screen.getByRole('heading', { name: title })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /Qidi Filament · 2 Einheiten · 5 Fächer/ })).toHaveAttribute('aria-current', 'true');
  expect(screen.queryByRole('button', { name: /1 · PLA/ })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: /Saturn Resin/ }));
  expect(within(general()).getByLabelText('Name')).toHaveValue('Saturn');
  expect(screen.queryByLabelText('Düse (mm)')).toBeNull();
  expect(screen.getByText('Für Resin-Drucker gibt es noch keine Druckeranbindung.')).toBeInTheDocument();
  await screen.findByText('Noch keine Drucke von diesem Drucker.');
  expect(within(units()).queryByRole('button', { name: /Einheit hinzufügen/ })).toBeNull();
});

it('selects the navigation context and opens Material Manager with the same ID', async () => {
  const { onMaterial } = setup('2');
  fireEvent.click(await screen.findByRole('button', { name: 'Spulen einlegen im Material Manager →' }));
  expect(onMaterial).toHaveBeenCalledWith({ printerId: '2' });
});

it('validates nozzle and bed limits, saves all fields and renames, then allows clearing', async () => {
  setup(); await screen.findByText('1 · PLA · Rot');
  fireEvent.click(within(general()).getByRole('button', { name: 'Bearbeiten' }));
  const input = (name: string, value: string) => fireEvent.change(screen.getByLabelText(name), { target: { value } });
  input('Düse (mm)', '2,1');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  expect(screen.getByRole('alert')).toHaveTextContent('Düse muss zwischen 0,1 und 2,0 mm liegen.');
  expect(invoke).not.toHaveBeenCalledWith('update_printer_details', expect.anything());
  input('Düse (mm)', '0,4'); input('Bauraum X × Y × Z (mm) X', '0');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  expect(screen.getByRole('alert')).toHaveTextContent('Bauraum muss je Achse');
  input('Name', 'Qidi Plus'); input('Hersteller', ' Qidi '); input('Modell', 'Plus4');
  input('Bauraum X × Y × Z (mm) X', '305'); input('Bauraum X × Y × Z (mm) Y', '305'); input('Bauraum X × Y × Z (mm) Z', '280');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('update_printer_details', { id: '1', details: { manufacturer: 'Qidi', model: 'Plus4', nozzleMm: 0.4, bedXMm: 305, bedYMm: 305, bedZMm: 280 } }));
  await waitFor(() => expect(within(general()).getByRole('button', { name: 'Bearbeiten' })).toBeInTheDocument());
  expect(screen.getByLabelText('Name')).toHaveValue('Qidi Plus');
  fireEvent.click(within(general()).getByRole('button', { name: 'Bearbeiten' }));
  for (const label of ['Hersteller', 'Modell', 'Düse (mm)', 'Bauraum X × Y × Z (mm) X', 'Bauraum X × Y × Z (mm) Y', 'Bauraum X × Y × Z (mm) Z']) input(label, '');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('update_printer_details', { id: '1', details }));
});

it('cancels edits and keeps failed edits visible with an error', async () => {
  setup(); await screen.findByText('1 · PLA · Rot');
  fireEvent.click(within(general()).getByRole('button', { name: 'Bearbeiten' }));
  fireEvent.change(screen.getByLabelText('Hersteller'), { target: { value: 'draft' } });
  fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
  expect(screen.getByLabelText('Hersteller')).toHaveValue('');
  fireEvent.click(within(general()).getByRole('button', { name: 'Bearbeiten' }));
  vi.mocked(invoke).mockRejectedValueOnce('Cannot save');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  await waitFor(() => expect(screen.getAllByRole('alert').some(el => el.textContent?.includes('Cannot save'))).toBe(true));
  expect(screen.getByRole('button', { name: 'Speichern' })).toBeInTheDocument();
});

it('deletes only after confirmation with return-home count and supports Escape, focus return and Tab trap', async () => {
  setup(); await screen.findByText('1 · PLA · Rot');
  const trigger = screen.getByRole('button', { name: 'Drucker löschen' }); trigger.focus(); fireEvent.click(trigger);
  const dialog = screen.getByRole('dialog');
  expect(dialog).toHaveTextContent('1 Spule');
  expect(document.activeElement).toBe(within(dialog).getByRole('button', { name: 'Abbrechen' }));
  fireEvent.keyDown(document.activeElement!, { key: 'Tab', shiftKey: true });
  expect(within(dialog).getByRole('button', { name: 'Drucker löschen' })).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
  expect(screen.queryByRole('dialog')).toBeNull(); expect(trigger).toHaveFocus();
  expect(invoke).not.toHaveBeenCalledWith('delete_printer', expect.anything());
  fireEvent.click(trigger);
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Drucker löschen' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('delete_printer', { printerId: '1' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(screen.getByLabelText('Name')).toHaveValue('Saturn');
});

it('renders history states and routes open jobs to the review dialog', async () => {
  history = ['open', 'confirmed', 'ignored'].map((state, i) => ({ id: String(i), fileName: `job${i}.gcode`, outcome: 'completed', printDurationS: 120, usedMm: 100, grams: state === 'confirmed' ? 18.4 : null, state } as PrinterHistoryJob));
  const { onMaterial } = setup();
  expect(await screen.findByText('job2.gcode')).toBeInTheDocument();
  expect(screen.getByText('gebucht')).toBeInTheDocument(); expect(screen.getByText('ignoriert')).toBeInTheDocument();
  expect(screen.getByText('18,4 g')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Im Material Manager bestätigen' }));
  expect(onMaterial).toHaveBeenCalledWith({ printerId: '1', reviewJobs: true });
});

it('shows connection controls only when enabled and clears them on resin selection', async () => {
  setup(undefined, { ...link(), enabled: true });
  expect(await screen.findByRole('button', { name: 'Verbindung testen' })).toBeInTheDocument();
  expect(screen.getByText('Später kommen hier OctoPrint, Bambu und Prusa als weitere Typen dazu.')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: /Saturn Resin/ }));
  expect(screen.queryByRole('button', { name: 'Verbindung testen' })).toBeNull();
});

it('reorders printers with mouse threshold and keyboard, and units by dragging', async () => {
  setup(); await screen.findByText('1 · PLA · Rot');
  const handle = screen.getByRole('button', { name: /Zum Sortieren.*Qidi/ });
  fireEvent.mouseDown(handle, { button: 0, clientX: 0, clientY: 0 });
  fireEvent.mouseMove(document, { clientX: 2, clientY: 0 });
  fireEvent.mouseEnter(screen.getByRole('button', { name: /Saturn Resin/ }).parentElement!.parentElement!);
  fireEvent.mouseUp(document);
  expect(invoke).not.toHaveBeenCalledWith('reorder_printers', expect.anything());
  fireEvent.mouseDown(handle, { button: 0, clientX: 0, clientY: 0 });
  fireEvent.mouseMove(document, { clientX: 20, clientY: 0 });
  fireEvent.mouseEnter(screen.getByRole('button', { name: /Saturn Resin/ }).parentElement!.parentElement!);
  fireEvent.mouseUp(document);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('reorder_printers', { ids: ['2', '1'] }));
  fireEvent.keyDown(handle, { key: 'ArrowDown' });
  const unit = screen.getByRole('button', { name: /Zum Sortieren.*AMS A/ });
  fireEvent.mouseDown(unit, { button: 0, clientX: 0, clientY: 0 });
  fireEvent.mouseMove(document, { clientX: 20, clientY: 0 });
  fireEvent.mouseEnter(screen.getByRole('button', { name: /Zum Sortieren.*Holder/ }).parentElement!.parentElement!);
  fireEvent.mouseUp(document);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('reorder_units', { printerId: '1', unitIds: ['u2', 'u1'] }));
});

describe('unit management replacing PrinterManagePanel', () => {
  it('preserves the naming suggestions for multiple AMS and external holders', () => {
    expect(suggestUnitName(a, 'bambu_ams', 'AMS')).toBe('AMS B');
    expect(suggestUnitName(a, 'external', 'Spulenhalter')).toBe('Spulenhalter 2');
  });
  it('adds custom units with name and slots, and cancels with Escape', async () => {
    setup(); await screen.findByText('1 · PLA · Rot');
    fireEvent.click(screen.getByRole('button', { name: '+ Einheit hinzufügen' }));
    let dialog = screen.getByRole('dialog');
    fireEvent.click(within(dialog).getByRole('button', { name: /Eigene/ }));
    fireEvent.change(within(dialog).getByLabelText('Name'), { target: { value: 'Qidi Box' } });
    fireEvent.change(within(dialog).getByLabelText('Fächer'), { target: { value: '6' } });
    fireEvent.submit(dialog.querySelector('form')!);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('add_unit', { printerId: '1', kind: 'custom', name: 'Qidi Box', slotCount: 6 }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    fireEvent.click(screen.getByRole('button', { name: '+ Einheit hinzufügen' }));
    dialog = screen.getByRole('dialog'); fireEvent.keyDown(dialog, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
  });
  it('renames units and confirms their deletion with the spool return count', async () => {
    setup(); await screen.findByText('1 · PLA · Rot');
    fireEvent.click(within(units()).getAllByRole('button', { name: 'Bearbeiten' })[0]);
    fireEvent.change(within(screen.getByRole('dialog')).getByLabelText('Name'), { target: { value: 'AMS 1' } });
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('update_unit', { unitId: 'u1', name: 'AMS 1', slotCount: null }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    fireEvent.click(screen.getByRole('button', { name: 'Löschen AMS A' }));
    expect(screen.getByRole('dialog')).toHaveTextContent('1 Spule');
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Löschen' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('delete_unit', { unitId: 'u1' }));
  });
});

it.each([
  ['Bambu AMS', 'bambu_ams', 'AMS B'],
  ['Spulenhalter', 'external', 'Spulenhalter 2'],
])('adds the %s template with its suggested name', async (label, kind, name) => {
  setup(); await screen.findByText('1 · PLA · Rot');
  fireEvent.click(screen.getByRole('button', { name: '+ Einheit hinzufügen' }));
  const dialog = screen.getByRole('dialog');
  fireEvent.click(within(dialog).getByRole('button', { name: `${label} · ${kind === 'external' ? 1 : 4}` }));
  expect(within(dialog).getByLabelText('Name')).toHaveValue(name);
  fireEvent.submit(dialog.querySelector('form')!);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('add_unit', { printerId: '1', kind, name, slotCount: null }));
});

it('omits the return-home sentence for empty units and keeps Escape non-destructive', async () => {
  setup(); await screen.findByText('1 · PLA · Rot');
  fireEvent.click(screen.getByRole('button', { name: 'Löschen Holder' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).not.toMatch(/Stammplatz|Spule/);
  fireEvent.keyDown(dialog, { key: 'Escape' });
  expect(invoke).not.toHaveBeenCalledWith('delete_unit', expect.anything());
});

it('edits custom slot counts through the modal and does not allow fixed vat changes', async () => {
  list[0].units[0] = { ...list[0].units[0], kind: 'custom', name: 'Box' };
  setup(); await screen.findByText('1 · PLA · Rot');
  fireEvent.click(within(units()).getAllByRole('button', { name: 'Bearbeiten' })[0]);
  fireEvent.change(screen.getByLabelText('Fächer'), { target: { value: '2' } });
  fireEvent.submit(screen.getByRole('dialog').querySelector('form')!);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('update_unit', { unitId: 'u1', name: 'Box', slotCount: 2 }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  fireEvent.click(screen.getByRole('button', { name: /Saturn Resin/ }));
  await waitFor(() => expect(within(units()).getByRole('button', { name: 'Spulen einlegen im Material Manager →' })).toBeInTheDocument());
  expect(within(units()).getAllByRole('button')).toHaveLength(1);
});
