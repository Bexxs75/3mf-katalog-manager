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
const link = (): PrinterLinkState => ({ refreshKey: 0, enabled: false, connections: [], jobs: [], error: null,
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
const general = () => screen.getByRole('heading', { name: 'Hardware & Bauraum' }).closest('section')!;
const units = () => screen.getByRole('heading', { name: 'Materialzuordnung' }).closest('section')!;

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

it('shows master, detail cards and read-only slots; selects the other printer', async () => {
  setup();
  await screen.findByTitle('1 · PLA · Rot');
  for (const title of ['Qidi', 'Hardware & Bauraum', 'Anbindung & Netzwerk', 'Materialzuordnung']) expect(screen.getByRole('heading', { name: title })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /Qidi Inaktiv/ })).toHaveAttribute('aria-current', 'true');
  expect(screen.queryByRole('button', { name: /1 · PLA/ })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: /Saturn Inaktiv/ }));
  expect(within(general()).getByLabelText('Name')).toHaveValue('Saturn');
  expect(screen.queryByLabelText('Düse')).toBeNull();
  expect(screen.getByText('Für Resin-Drucker gibt es noch keine Druckeranbindung.')).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: 'Letzte Drucke' })).toBeNull();
  expect(within(units()).queryByRole('button', { name: /Einheit hinzufügen/ })).toBeNull();
});

it('selects the navigation context and opens Material Manager with the same ID', async () => {
  const { onMaterial } = setup('2');
  fireEvent.click(await screen.findByRole('button', { name: 'Spulen im Material Manager einlegen →' }));
  expect(onMaterial).toHaveBeenCalledWith({ printerId: '2' });
});

it('replaces the Edit button by a new Save button element so the opening click cannot submit the form', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
  const edit = within(screen.getByTestId('printer-detail-header')).getByRole('button', { name: 'Bearbeiten' });
  fireEvent.click(edit);
  const save = screen.getByRole('button', { name: 'Speichern' });
  expect(save).not.toBe(edit);
  expect(edit.isConnected).toBe(false);
});

it('validates nozzle and bed limits, saves all fields and renames, then allows clearing', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
  fireEvent.click(within(screen.getByTestId('printer-detail-header')).getByRole('button', { name: 'Bearbeiten' }));
  const input = (name: string, value: string) => fireEvent.change(screen.getByLabelText(name), { target: { value } });
  input('Düse', '2,1');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  expect(screen.getByRole('alert')).toHaveTextContent('Düse muss zwischen 0,1 und 2,0 mm liegen.');
  expect(invoke).not.toHaveBeenCalledWith('update_printer_details', expect.anything());
  input('Düse', '0,4'); input('Bauraum X × Y × Z X', '0');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  expect(screen.getByRole('alert')).toHaveTextContent('Bauraum muss je Achse');
  input('Name', 'Qidi Plus'); input('Hersteller', ' Qidi '); input('Modell', 'Plus4');
  input('Bauraum X × Y × Z X', '305'); input('Bauraum X × Y × Z Y', '305'); input('Bauraum X × Y × Z Z', '280');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('update_printer_details', { id: '1', details: { manufacturer: 'Qidi', model: 'Plus4', nozzleMm: 0.4, bedXMm: 305, bedYMm: 305, bedZMm: 280 } }));
  await waitFor(() => expect(within(screen.getByTestId('printer-detail-header')).getByRole('button', { name: 'Bearbeiten' })).toBeInTheDocument());
  expect(screen.getByLabelText('Name')).toHaveValue('Qidi Plus');
  fireEvent.click(within(screen.getByTestId('printer-detail-header')).getByRole('button', { name: 'Bearbeiten' }));
  for (const label of ['Hersteller', 'Modell', 'Düse', 'Bauraum X × Y × Z X', 'Bauraum X × Y × Z Y', 'Bauraum X × Y × Z Z']) input(label, '');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('update_printer_details', { id: '1', details }));
});

it('cancels edits and keeps failed edits visible with an error', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
  fireEvent.click(within(screen.getByTestId('printer-detail-header')).getByRole('button', { name: 'Bearbeiten' }));
  fireEvent.change(screen.getByLabelText('Hersteller'), { target: { value: 'draft' } });
  fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
  expect(screen.getByLabelText('Hersteller')).toHaveValue('');
  fireEvent.click(within(screen.getByTestId('printer-detail-header')).getByRole('button', { name: 'Bearbeiten' }));
  vi.mocked(invoke).mockRejectedValueOnce('Cannot save');
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  await waitFor(() => expect(screen.getAllByRole('alert').some(el => el.textContent?.includes('Cannot save'))).toBe(true));
  expect(screen.getByRole('button', { name: 'Speichern' })).toBeInTheDocument();
});

it('deletes only after confirmation with return-home count and supports Escape, focus return and Tab trap', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
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
  expect(await screen.findByRole('button', { name: 'Verbindung einrichten' })).toBeInTheDocument();
  expect(screen.getByText('Später kommen hier OctoPrint, Bambu und Prusa als weitere Typen dazu.')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: /Saturn Inaktiv/ }));
  expect(screen.queryByRole('button', { name: 'Verbindung einrichten' })).toBeNull();
});

it('reorders printers with mouse threshold and keyboard, and units by dragging', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
  const handle = screen.getByRole('button', { name: /Zum Sortieren.*Qidi/ });
  fireEvent.mouseDown(handle, { button: 0, clientX: 0, clientY: 0 });
  fireEvent.mouseMove(document, { clientX: 2, clientY: 0 });
  fireEvent.mouseEnter(screen.getByRole('button', { name: /Saturn Inaktiv/ }).parentElement!.parentElement!);
  fireEvent.mouseUp(document);
  expect(invoke).not.toHaveBeenCalledWith('reorder_printers', expect.anything());
  fireEvent.mouseDown(handle, { button: 0, clientX: 0, clientY: 0 });
  fireEvent.mouseMove(document, { clientX: 20, clientY: 0 });
  fireEvent.mouseEnter(screen.getByRole('button', { name: /Saturn Inaktiv/ }).parentElement!.parentElement!);
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
    setup(); await screen.findByTitle('1 · PLA · Rot');
    fireEvent.click(screen.getByRole('button', { name: 'Einheit hinzufügen' }));
    let dialog = screen.getByRole('dialog');
    fireEvent.click(within(dialog).getByRole('button', { name: /Eigene/ }));
    fireEvent.change(within(dialog).getByLabelText('Name'), { target: { value: 'Qidi Box' } });
    fireEvent.change(within(dialog).getByLabelText('Fächer'), { target: { value: '6' } });
    fireEvent.submit(dialog.querySelector('form')!);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('add_unit', { printerId: '1', kind: 'custom', name: 'Qidi Box', slotCount: 6 }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    fireEvent.click(screen.getByRole('button', { name: 'Einheit hinzufügen' }));
    dialog = screen.getByRole('dialog'); fireEvent.keyDown(dialog, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
  });
  it('renames units and confirms their deletion with the spool return count', async () => {
    setup(); await screen.findByTitle('1 · PLA · Rot');
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
  setup(); await screen.findByTitle('1 · PLA · Rot');
  fireEvent.click(screen.getByRole('button', { name: 'Einheit hinzufügen' }));
  const dialog = screen.getByRole('dialog');
  fireEvent.click(within(dialog).getByRole('button', { name: `${label} · ${kind === 'external' ? 1 : 4}` }));
  expect(within(dialog).getByLabelText('Name')).toHaveValue(name);
  fireEvent.submit(dialog.querySelector('form')!);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('add_unit', { printerId: '1', kind, name, slotCount: null }));
});

it('omits the return-home sentence for empty units and keeps Escape non-destructive', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
  fireEvent.click(screen.getByRole('button', { name: 'Löschen Holder' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).not.toMatch(/Stammplatz|Spule/);
  fireEvent.keyDown(dialog, { key: 'Escape' });
  expect(invoke).not.toHaveBeenCalledWith('delete_unit', expect.anything());
});

it('edits custom slot counts through the modal and does not allow fixed vat changes', async () => {
  list[0].units[0] = { ...list[0].units[0], kind: 'custom', name: 'Box' };
  setup(); await screen.findByTitle('1 · PLA · Rot');
  fireEvent.click(within(units()).getAllByRole('button', { name: 'Bearbeiten' })[0]);
  fireEvent.change(screen.getByLabelText('Fächer'), { target: { value: '2' } });
  fireEvent.submit(screen.getByRole('dialog').querySelector('form')!);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('update_unit', { unitId: 'u1', name: 'Box', slotCount: 2 }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  fireEvent.click(screen.getByRole('button', { name: /Saturn Inaktiv/ }));
  await waitFor(() => expect(within(units()).getByRole('button', { name: 'Spulen im Material Manager einlegen →' })).toBeInTheDocument());
  expect(within(units()).getAllByRole('button')).toHaveLength(1);
});

it('keeps material and color names in slot titles and deletion in the detail header', async () => {
  setup();
  const slot = await screen.findByTitle('1 · PLA · Rot');
  expect(slot).toHaveClass('w-6', 'h-6');
  expect(within(units()).getByTitle('2 · leer')).toHaveAttribute('title', '2 · leer');
  expect(within(screen.getByTestId('printer-detail-header')).getByRole('button', { name: 'Drucker löschen' })).toBeInTheDocument();
});

it('puts the switch in the master footer and keeps the API key disabled', async () => {
  setup();
  await screen.findByTitle('1 · PLA · Rot');
  const master = screen.getByRole('complementary', { name: 'Drucker' });
  expect(within(master.querySelector('footer')!).getByRole('switch', { name: 'Anbindung' })).toHaveAttribute('aria-checked', 'false');
  expect(screen.getByLabelText('API-Schlüssel')).toBeDisabled();
  expect(screen.getByPlaceholderText('Bei Klipper nicht nötig')).toBeDisabled();
  expect(screen.getByText(/Schalte unten in der Druckerliste/)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Verbindung einrichten' })).toBeDisabled();
  expect(screen.queryByRole('heading', { name: 'Printer Manager' })).toBeNull();
});

it('uses viewport breakpoints for the icon rail and closes the overlay with Escape, backdrop and selection', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
  const master = screen.getByRole('complementary');
  const root = master.parentElement!;
  const menu = within(master).getByRole('button', { name: 'Druckerliste ein- oder ausklappen' });
  expect(root).toHaveClass('grid-cols-[var(--pm-master-width)_minmax(0,1fr)]');
  expect(screen.getByRole('main')).toHaveClass('col-start-2');
  fireEvent.click(menu);
  expect(menu).toHaveAttribute('aria-expanded', 'true');
  expect(master).toHaveClass('max-[1023px]:w-[300px]');
  fireEvent.keyDown(document, { key: 'Escape' });
  expect(menu).toHaveAttribute('aria-expanded', 'false');
  expect(menu).toHaveFocus();
  fireEvent.click(menu);
  fireEvent.click(screen.getAllByRole('button', { name: 'Druckerliste ein- oder ausklappen' }).find(b => !master.contains(b))!);
  expect(menu).toHaveAttribute('aria-expanded', 'false');
  fireEvent.click(menu);
  fireEvent.click(screen.getByRole('button', { name: 'Saturn Inaktiv' }));
  expect(menu).toHaveAttribute('aria-expanded', 'false');
  expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Saturn');
});

it('selects printers with arrow keys without reordering them', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
  fireEvent.keyDown(screen.getByRole('button', { name: 'Qidi Inaktiv' }), { key: 'ArrowDown' });
  const saturn = screen.getByRole('button', { name: 'Saturn Inaktiv' });
  expect(saturn).toHaveAttribute('aria-current', 'true');
  expect(saturn).toHaveFocus();
  expect(invoke).not.toHaveBeenCalledWith('reorder_printers', expect.anything());
  fireEvent.keyDown(saturn, { key: 'ArrowUp' });
  expect(screen.getByRole('button', { name: 'Qidi Inaktiv' })).toHaveAttribute('aria-current', 'true');
});

it.each([
  [true, false, null, 'Verbunden'],
  [true, true, null, 'Fehler'],
  [true, false, 'unreachable', 'Fehler'],
  [false, false, null, 'Inaktiv'],
] as const)('shows effective connection status enabled=%s paused=%s error=%s', async (enabled, paused, lastError, status) => {
  const printerLink = { ...link(), enabled, connections: [{ printerId: '1', kind: 'moonraker' as const, address: '192.168.1.60', baseUrl: 'http://192.168.1.60:7125', remoteVersion: 'v0.8', connectedSince: 1, lastSyncedAt: 2, lastError, errorSince: null, paused, clockOffsetS: 0 }] };
  setup(undefined, printerLink); await screen.findByTitle('1 · PLA · Rot');
  const entry = screen.getByRole('button', { name: `Qidi ${status}` });
  expect(entry).toHaveAttribute('title', `Qidi · ${status}`);
  expect(entry.parentElement!.parentElement!).toHaveClass('bg-[var(--sel)]', 'border-[var(--sel-line)]');
  expect(entry.parentElement!.parentElement!).not.toHaveClass('bg-[var(--accent-soft)]');
  expect(within(entry).getByText('192.168.1.60')).toBeInTheDocument();
  expect(within(screen.getByTestId('printer-detail-header')).getByText(status)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Saturn Inaktiv' })).toBeInTheDocument();
});

it('rejects invalid spool colors and keeps empty slots hatched', async () => {
  const badSpool = { ...spool, colorHex: 'url(https://invalid.example)' };
  vi.mocked(invoke).mockImplementation(async cmd => {
    if (cmd === 'list_printers') return list;
    if (cmd === 'list_filament_spools') return [badSpool];
    if (cmd === 'list_printer_history') return [];
  });
  setup();
  expect(await screen.findByTitle('1 · PLA · Rot')).toHaveStyle({ background: 'var(--unk-soft)' });
  expect(screen.getByTitle('2 · leer').getAttribute('style')).toContain('repeating-linear-gradient');
  expect(screen.getByText(/4 Fächer · 1 belegt/)).toBeInTheDocument();
});

it('keeps the list open when Escape closes its add-printer dialog', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
  const master = screen.getByRole('complementary');
  const menu = within(master).getByRole('button', { name: 'Druckerliste ein- oder ausklappen' });
  fireEvent.click(menu);
  fireEvent.click(within(master).getByRole('button', { name: '+ Drucker hinzufügen' }));
  fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(menu).toHaveAttribute('aria-expanded', 'true');
});

it('reorders units using the keyboard handle', async () => {
  setup(); await screen.findByTitle('1 · PLA · Rot');
  fireEvent.keyDown(screen.getByRole('button', { name: /Zum Sortieren.*AMS A/ }), { key: 'ArrowDown' });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('reorder_units', { printerId: '1', unitIds: ['u2', 'u1'] }));
});

it('follows numeric and alphabetic unit naming and skips collisions', () => {
  const numeric = {...a, units: [{...a.units[0], name: 'AMS 1'}]};
  expect(suggestUnitName(numeric, 'bambu_ams', 'AMS')).toBe('AMS 2');
  expect(suggestUnitName({...numeric, units: [...numeric.units, {...a.units[0], name: 'AMS 2'}]}, 'bambu_ams', 'AMS')).toBe('AMS 3');
  expect(suggestUnitName(a, 'bambu_ams', 'AMS')).toBe('AMS B');
  expect(suggestUnitName({...a, units: []}, 'bambu_ams', 'AMS')).toBe('AMS 1');
});
it.each([['de', '0,4', '305,5'], ['fr', '0,4', '305,5'], ['es', '0,4', '305,5'], ['en', '0.4', '305.5']])('formats printer decimals in %s', async (language, nozzle, bed) => {
  localStorage.setItem('3mf-katalog-language', language);
  list[0].nozzleMm = 0.4; list[0].bedXMm = 305.5;
  setup();
  await waitFor(() => expect(document.querySelector('input[value="'+nozzle+'"]')).not.toBeNull());
  expect(document.querySelector('input[value="'+bed+'"]')).not.toBeNull();
});
