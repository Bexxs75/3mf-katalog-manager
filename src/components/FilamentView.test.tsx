import type { ReactNode } from 'react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { FilamentView } from './FilamentView';
import type { FilamentSpool, Printer, PrinterJob } from '../types';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import { usePrinters } from '../hooks/usePrinters';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => {}) }),
}));

function spool(overrides: Partial<FilamentSpool>): FilamentSpool {
  return {
    id: 's1', material: 'PLA', manufacturer: null, color: 'Schwarz', location: null,
    diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 620, price: null, imagePng: null,
    colorHex: '#1a1a1a', colorHexes: [], homeLocation: null, unitId: null, slotIndex: null, kind: 'filament', ...overrides,
  };
}

const LOADED = spool({ id: 'in', homeLocation: 'Regal 2', unitId: 'u1', slotIndex: 0 });
const STORED = spool({ id: 'store', material: 'PETG', color: 'Rot', location: 'Regal 1', colorHex: '#c0392b' });
const RESIN = spool({
  id: 'resin1', kind: 'resin', material: 'Standard', manufacturer: 'Elegoo', color: 'Grau',
  location: 'Resin-Schrank', colorHex: '#8a8f98', originalWeightG: 1000, remainingWeightG: 640.5,
});
const X1C: Printer = {
  id: 'p1',
  name: 'X1C',
  kind: 'filament',
  manufacturer: null, model: null, nozzleMm: null, bedXMm: null, bedYMm: null, bedZMm: null, units: [{ id: 'u1', printerId: 'p1', name: 'AMS A', kind: 'bambu_ams', slotCount: 4, bambuAmsIndex: 0 }],
};

const SATURN: Printer = {
  id: 'p9',
  name: 'Saturn 4',
  kind: 'resin',
  manufacturer: null, model: null, nozzleMm: null, bedXMm: null, bedYMm: null, bedZMm: null, units: [{ id: 'vat', printerId: 'p9', name: 'Harzwanne', kind: 'resin_vat', slotCount: 1, bambuAmsIndex: null }],
};

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === 'list_filament_spools') return Promise.resolve([LOADED, STORED, RESIN]);
    if (cmd === 'list_printers') return Promise.resolve([X1C]);
    if (cmd === 'unload_spool') return Promise.resolve('Regal 2');
    if (cmd === 'load_spool') return Promise.resolve({ displacedSpoolId: null });
    if (cmd === 'list_file_summaries') return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.removeItem('3mf-katalog-filament-kind');
  localStorage.removeItem('3mf-katalog-filament-layout');
  localStorage.removeItem('3mf-katalog-filament-group-mode');
});

function printerLink(): PrinterLinkState {
  return {
    enabled: false, connections: [], jobs: [], error: null,
    refresh: vi.fn(), setEnabled: vi.fn(), testConnection: vi.fn(),
    removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn(),
  } as unknown as PrinterLinkState;
}

const printerJob = (over: Partial<PrinterJob>): PrinterJob => ({
  id: '1', printerId: 'p1', printerName: 'Sovol SV08', fileName: 'Test.gcode', outcome: 'completed',
  rawStatus: 'completed', endedAt: 1700000000, printDurationS: 120, usedMm: 100, partialPercent: null,
  material: 'PLA', hasThumbnail: false, suggestedSpoolId: null, grams: null, materialMismatch: false,
  modelMatch: null, ...over,
});

function JobsWrapper({ jobs }: { jobs: PrinterJob[] }) {
  const link = { ...printerLink(), enabled: true, jobs };
  return (
    <LanguageProvider>
      <PrintersWrapper>{(printers) => <FilamentView printerLink={link} printers={printers} />}</PrintersWrapper>
    </LanguageProvider>
  );
}

// Mirrors App.tsx: a single `usePrinters()` instance passed to FilamentView
// (and, in the real app, to Rail as well).
function PrintersWrapper({ children }: { children: (printers: ReturnType<typeof usePrinters>) => ReactNode }) {
  const printers = usePrinters();
  return <>{children(printers)}</>;
}

function renderView() {
  return render(
    <LanguageProvider>
      <PrintersWrapper>{(printers) => <FilamentView printerLink={printerLink()} printers={printers} />}</PrintersWrapper>
    </LanguageProvider>,
  );
}

describe('FilamentView with printers', () => {
  it('remembers list layout after remounting', async () => {
    const first = renderView();
    await screen.findByTestId('spool-card-store');
    fireEvent.click(screen.getByRole('button', { name: 'Liste' }));
    expect(screen.getByRole('table')).toBeInTheDocument();
    first.unmount();
    renderView();
    expect(await screen.findByRole('table')).toBeInTheDocument();
  });

  it.each(['filament', 'resin'] as const)('suggests saved manufacturers only for %s', async (kind) => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') return Promise.resolve([
        { ...STORED, manufacturer: 'Custom Filament' },
        { ...RESIN, manufacturer: 'Custom Resin' },
      ]);
      if (cmd === 'list_printers') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    renderView();
    await screen.findByTestId('spool-card-store');
    if (kind === 'resin') fireEvent.click(screen.getByRole('button', { name: 'Resin' }));
    fireEvent.click(screen.getByRole('button', { name: kind === 'resin' ? 'Flasche anlegen' : 'Spule anlegen' }));
    const input = screen.getByPlaceholderText('Hersteller');
    fireEvent.change(input, { target: { value: 'Custom' } });
    expect(screen.getAllByRole('option').map(o => o.textContent)).toEqual([
      kind === 'resin' ? 'Custom Resin' : 'Custom Filament',
    ]);
  });

  it('shows loaded spools only in the printer column in the single view but counts all spools', async () => {
    localStorage.setItem('3mf-katalog-filament-group-mode', 'single');
    renderView();
    await waitFor(() => expect(screen.getByTestId('slot-u1-0')).toHaveTextContent('PLA · Schwarz'));
    const storage = screen.getByTestId('filament-storage');
    expect(within(storage).queryByTestId('spool-card-in')).toBeNull();
    expect(within(storage).getByTestId('spool-card-store')).toBeInTheDocument();
    expect(screen.getByText('Spulen gesamt').nextElementSibling).toHaveTextContent('2');
  });

  it('counts a loaded spool\'s home location in the storage-location stat', async () => {
    // LOADED has `location: null` (sits in a slot) but `homeLocation:
    // 'Regal 2'`; STORED lies under `location: 'Regal 1'` in storage. Both
    // locations must be counted, not just the one of STORED.
    renderView();
    await waitFor(() => screen.getByTestId('slot-u1-0'));
    expect(screen.getByText('Belegte Lagerplätze').nextElementSibling).toHaveTextContent('2');
  });

  it('unloads through the slot menu and shows where the spool went', async () => {
    renderView();
    await waitFor(() => screen.getByTestId('slot-u1-0'));
    fireEvent.click(screen.getByTestId('slot-u1-0'));
    fireEvent.click(screen.getByRole('button', { name: 'Herausnehmen' }));
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('zurück nach Regal 2'));
    expect(invoke).toHaveBeenCalledWith('unload_spool', { spoolId: 'in', location: null });
  });

  it('loads a spool by dragging its card onto a slot', async () => {
    renderView();
    const card = await screen.findByTestId('spool-card-store');
    fireEvent.mouseDown(card, { clientX: 0, clientY: 0, button: 0 });
    fireEvent.mouseMove(document, { clientX: 40, clientY: 40 });
    fireEvent.mouseEnter(screen.getByTestId('slot-u1-2'));
    fireEvent.mouseUp(document);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('load_spool', { spoolId: 'store', unitId: 'u1', slotIndex: 2 }),
    );
  });

  it('unloads a spool by dragging it from its slot onto the storage area', async () => {
    renderView();
    const slot = await screen.findByTestId('slot-u1-0');
    await waitFor(() => expect(slot).toHaveTextContent('PLA'));
    fireEvent.mouseDown(slot, { clientX: 0, clientY: 0, button: 0 });
    fireEvent.mouseMove(document, { clientX: 40, clientY: 40 });
    fireEvent.mouseEnter(screen.getByTestId('filament-storage'));
    fireEvent.mouseUp(document);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('unload_spool', { spoolId: 'in', location: null }));
  });

  it('restocks a spool: one backend call, highlighted new card, confirmation toast', async () => {
    const NEW = spool({ id: 'new1', material: 'PETG', color: 'Rot', location: 'Regal 1', colorHex: '#c0392b', remainingWeightG: 1000 });
    let list = [LOADED, STORED];
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') return Promise.resolve(list);
      if (cmd === 'list_printers') return Promise.resolve([X1C]);
      if (cmd === 'restock_filament_spool') {
        list = [LOADED, STORED, NEW];
        return Promise.resolve([NEW]);
      }
      return Promise.resolve(undefined);
    });
    renderView();
    const card = await screen.findByTestId('spool-card-store');
    fireEvent.click(within(card).getByRole('button', { name: 'Nachkaufen' }));
    fireEvent.click(await screen.findByRole('button', { name: '1 Spule anlegen' }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('restock_filament_spool', {
        templateId: 'store', count: 1, weight: 1000, price: null, location: 'Regal 1',
      }),
    );
    // The new spool equals STORED, so both sit in one group card, which carries the highlight.
    expect(await screen.findByTestId('group-card-store')).toHaveClass('spool-new');
    expect(screen.getByRole('status')).toHaveTextContent('1 Spule PETG · Rot angelegt');
    expect(screen.queryByRole('dialog', { name: /Nachkaufen/ })).toBeNull();
  });

  it('shows only filament by default and switches to resin with its own stats', async () => {
    renderView();
    await screen.findByTestId('spool-card-store');
    expect(screen.queryByTestId('spool-card-resin1')).toBeNull();
    expect(screen.getByText('Spulen gesamt').nextElementSibling).toHaveTextContent('2');

    fireEvent.click(screen.getByRole('button', { name: 'Resin' }));

    expect(await screen.findByTestId('spool-card-resin1')).toBeInTheDocument();
    expect(screen.queryByTestId('spool-card-store')).toBeNull();
    expect(screen.getByText('Flaschen gesamt').nextElementSibling).toHaveTextContent('1');
    expect(screen.getByText('Restbestand gesamt').nextElementSibling).toHaveTextContent('640,5 ml');
    // In the resin view the printer column shows only resin printers.
    expect(screen.queryByTestId('slot-u1-0')).toBeNull();
  });

  it('deducts resin and confirms the amount', async () => {
    const after = { ...RESIN, remainingWeightG: 595 };
    let list = [LOADED, STORED, RESIN];
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') return Promise.resolve(list);
      if (cmd === 'list_printers') return Promise.resolve([X1C]);
      if (cmd === 'consume_resin') {
        list = [LOADED, STORED, after];
        return Promise.resolve(after);
      }
      return Promise.resolve(undefined);
    });
    renderView();
    fireEvent.click(await screen.findByRole('button', { name: 'Resin' }));
    const card = await screen.findByTestId('spool-card-resin1');
    fireEvent.click(within(card).getByRole('button', { name: 'Verbrauch' }));
    fireEvent.change(await screen.findByLabelText('Verbraucht (ml)'), { target: { value: '45,5' } });
    fireEvent.click(screen.getByRole('button', { name: 'Abbuchen' }));
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('45,5 ml von Standard · Grau abgebucht'));
    expect(await screen.findByTestId('spool-card-resin1')).toHaveTextContent('595 ml');
  });

  it('remembers the chosen kind', async () => {
    const first = render(
      <LanguageProvider>
        <PrintersWrapper>{(printers) => <FilamentView printerLink={printerLink()} printers={printers} />}</PrintersWrapper>
      </LanguageProvider>,
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Resin' }));
    first.unmount();
    renderView();
    expect(await screen.findByTestId('spool-card-resin1')).toBeInTheDocument();
  });

  it('never offers resin for a printer slot', async () => {
    renderView();
    const slot = await screen.findByTestId('slot-u1-2');
    fireEvent.click(slot);
    const menu = await screen.findByRole('menu');
    expect(within(menu).getByText('PETG · Rot')).toBeInTheDocument();
    expect(within(menu).queryByText(/Standard/)).toBeNull();
  });

  it('shows the resin printer only in the resin view and drags a bottle onto its vat', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') return Promise.resolve([LOADED, STORED, RESIN]);
      if (cmd === 'list_printers') return Promise.resolve([X1C, SATURN]);
      if (cmd === 'load_spool') return Promise.resolve({ displacedSpoolId: null });
      return Promise.resolve(undefined);
    });
    renderView();
    await screen.findByTestId('slot-u1-0');
    expect(screen.queryByText('Saturn 4')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Resin' }));
    const vat = await screen.findByTestId('slot-vat-0');
    expect(screen.queryByTestId('slot-u1-0')).toBeNull();
    const card = await screen.findByTestId('spool-card-resin1');
    fireEvent.mouseDown(card, { clientX: 0, clientY: 0, button: 0 });
    fireEvent.mouseMove(document, { clientX: 40, clientY: 40 });
    fireEvent.mouseEnter(vat);
    fireEvent.mouseUp(document);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('load_spool', { spoolId: 'resin1', unitId: 'vat', slotIndex: 0 }),
    );
  });

  it('takes a bottle out of the vat back to its home location', async () => {
    const inVat = { ...RESIN, location: null, homeLocation: 'Resin-Schrank', unitId: 'vat', slotIndex: 0 };
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') return Promise.resolve([STORED, inVat]);
      if (cmd === 'list_printers') return Promise.resolve([X1C, SATURN]);
      if (cmd === 'unload_spool') return Promise.resolve('Resin-Schrank');
      return Promise.resolve(undefined);
    });
    renderView();
    fireEvent.click(await screen.findByRole('button', { name: 'Resin' }));
    const vat = await screen.findByTestId('slot-vat-0');
    await waitFor(() => expect(vat).toHaveTextContent('Standard · Grau'));
    fireEvent.click(vat);
    fireEvent.click(screen.getByRole('button', { name: 'Herausnehmen' }));
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('zurück nach Resin-Schrank'));
    expect(invoke).toHaveBeenCalledWith('unload_spool', { spoolId: 'resin1', location: null });
  });

  it('does not drop a filament spool on a slot it does not fit (no request)', async () => {
    // Filament view: a resin printer isn't visible here at all; dropping
    // onto a spot without a matching target triggers nothing.
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') return Promise.resolve([STORED, RESIN]);
      if (cmd === 'list_printers') return Promise.resolve([SATURN]);
      return Promise.resolve(undefined);
    });
    renderView();
    const card = await screen.findByTestId('spool-card-store');
    expect(screen.queryByTestId('slot-vat-0')).toBeNull();
    fireEvent.mouseDown(card, { clientX: 0, clientY: 0, button: 0 });
    fireEvent.mouseMove(document, { clientX: 40, clientY: 40 });
    fireEvent.mouseUp(document);
    expect(invoke).not.toHaveBeenCalledWith('load_spool', expect.anything());
  });

  it('opens the edit form on a double-click on a storage card', async () => {
    renderView();
    const card = await screen.findByTestId('spool-card-store');
    fireEvent.doubleClick(within(card).getByText('PETG'));
    await waitFor(() => expect(screen.getByDisplayValue('Regal 1')).toBeInTheDocument());
  });

  it('shows the bottle wording on the add-panel button for resin', async () => {
    renderView();
    await screen.findByTestId('spool-card-store');
    expect(screen.getByRole('button', { name: 'Spule anlegen' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Flasche anlegen' })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Resin' }));

    expect(await screen.findByRole('button', { name: 'Flasche anlegen' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Spule anlegen' })).toBeNull();
  });

  it('shows the total remaining stock in whole grams, without decimals from the tenth-gram sum', async () => {
    // 620.3 g + 620.4 g = 1240.7 g, the statistics show whole grams.
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') {
        return Promise.resolve([
          spool({ id: 'in', homeLocation: 'Regal 2', unitId: 'u1', slotIndex: 0, remainingWeightG: 620.3 }),
          spool({ id: 'store', material: 'PETG', color: 'Rot', location: 'Regal 1', colorHex: '#c0392b', remainingWeightG: 620.4 }),
        ]);
      }
      if (cmd === 'list_printers') return Promise.resolve([X1C]);
      if (cmd === 'list_file_summaries') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    renderView();
    await waitFor(() => screen.getByTestId('slot-u1-0'));
    expect(screen.getByText('Restbestand gesamt').nextElementSibling).toHaveTextContent('1.241 g');
    expect(screen.getByText('Restbestand gesamt').nextElementSibling).not.toHaveTextContent(',');
  });
});

describe('FilamentView shares one printers instance with other consumers', () => {
  it('reflects a printer added elsewhere (e.g. the Rail "Drucker" tab) without a remount', async () => {
    // App.tsx passes the same usePrinters() instance to FilamentView AND Rail,
    // otherwise Rail would only see changes after a remount.
    let printerAdded = false;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') return Promise.resolve([LOADED, STORED]);
      if (cmd === 'list_printers') {
        return Promise.resolve(printerAdded ? [X1C, { id: 'p2', name: 'Neuer Drucker', kind: 'filament', manufacturer: null, model: null, nozzleMm: null, bedXMm: null, bedYMm: null, bedZMm: null, units: [] }] : [X1C]);
      }
      if (cmd === 'add_printer') {
        printerAdded = true;
        return Promise.resolve(undefined);
      }
      if (cmd === 'unload_spool') return Promise.resolve('Regal 2');
      if (cmd === 'load_spool') return Promise.resolve({ displacedSpoolId: null });
      return Promise.resolve(undefined);
    });

    render(
      <LanguageProvider>
        <PrintersWrapper>
          {(printers) => (
            <>
              {/* Stands in for a second consumer of the same instance, e.g. Rail. */}
              <div data-testid="other-consumer">{printers.printers.map((p) => p.name).join(', ')}</div>
              <button onClick={() => printers.addPrinter('Neuer Drucker', 'Halter', 'filament')}>Drucker hinzufuegen</button>
              <FilamentView printerLink={printerLink()} printers={printers} />
            </>
          )}
        </PrintersWrapper>
      </LanguageProvider>,
    );

    const printerColumn = () => screen.getByRole('complementary', { name: 'Drucker' });
    await waitFor(() => expect(screen.getByTestId('other-consumer')).toHaveTextContent('X1C'));
    expect(printerColumn()).toHaveTextContent('X1C');

    fireEvent.click(screen.getByRole('button', { name: 'Drucker hinzufuegen' }));

    await waitFor(() => expect(screen.getByTestId('other-consumer')).toHaveTextContent('Neuer Drucker'));
    // The same refresh must also reach FilamentView's own printer column.
    expect(printerColumn()).toHaveTextContent('Neuer Drucker');
  });
});

describe('FilamentView printer-jobs dialog state', () => {
  it('resets jobsOpen once the job list empties, so the banner returns and no dialog pops up unasked', async () => {
    const { rerender } = render(<JobsWrapper jobs={[printerJob({ id: '1' })]} />);
    await waitFor(() => screen.getByTestId('slot-u1-0'));

    fireEvent.click(screen.getByRole('button', { name: 'Prüfen' }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // The list empties (e.g. after the last confirm/ignore) -
    // without a reset the banner would stay hidden for good.
    rerender(<JobsWrapper jobs={[]} />);
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

    // New prints arrive - the dialog must NOT pop up again unasked,
    // the banner must be back instead.
    rerender(<JobsWrapper jobs={[printerJob({ id: '2' })]} />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Prüfen' })).toBeInTheDocument();
  });
});

it('navigates to Printer Manager with a printer ID without opening the old panel', async () => {
  const navigate = vi.fn();
  render(<LanguageProvider><PrintersWrapper>{printers => <FilamentView printerLink={printerLink()} printers={printers} onPrinterManager={navigate} />}</PrintersWrapper></LanguageProvider>);
  fireEvent.click(await screen.findByRole('button', { name: 'In der Druckerverwaltung bearbeiten' }));
  expect(navigate).toHaveBeenCalledWith('p1');
  expect(screen.queryByRole('dialog')).toBeNull();
});

it('switches to and focuses a resin printer when the shared list loads after navigation', async () => {
  vi.mocked(invoke).mockImplementation(async cmd => cmd === 'list_printers' ? [X1C, SATURN] : cmd === 'list_filament_spools' ? [RESIN] : []);
  render(<LanguageProvider><PrintersWrapper>{printers => <FilamentView printerLink={printerLink()} printers={printers} printerContext={{ printerId: 'p9' }} />}</PrintersWrapper></LanguageProvider>);
  await waitFor(() => expect(screen.getByLabelText('Saturn 4')).toHaveFocus());
  expect(screen.queryByTestId('slot-u1-0')).toBeNull();
});

it('opens the print review dialog immediately when navigated from history', async () => {
  const l = { ...printerLink(), enabled: true, jobs: [printerJob({})] };
  render(<LanguageProvider><PrintersWrapper>{printers => <FilamentView printerLink={l} printers={printers} printerContext={{ printerId: 'p1', reviewJobs: true }} />}</PrintersWrapper></LanguageProvider>);
  expect(await screen.findByRole('dialog', { name: 'Neue Drucke' })).toBeInTheDocument();
});

describe('FilamentView grouped inventory', () => {
  const G1 = spool({ id: 'g1', material: 'PLA', manufacturer: 'Polymaker', color: 'Schwarz', location: 'Regal A', remainingWeightG: 920 });
  const G2 = spool({ id: 'g2', material: 'PLA', manufacturer: 'Polymaker', color: 'Schwarz', location: 'Regal A', remainingWeightG: 640 });
  const LOW = spool({ id: 'low', material: 'PETG', manufacturer: 'Anycubic', color: 'Rot', colorHex: '#c0392b', location: 'Box', remainingWeightG: 120 });
  const FULL = spool({ id: 'full', material: 'TPU', manufacturer: 'eSun', color: 'Gelb', colorHex: '#f1c40f', location: 'Regal B', remainingWeightG: 1000 });
  const AMS1 = spool({ id: 'am1', material: 'PLA', manufacturer: 'Polymaker', color: 'Schwarz', location: null, homeLocation: 'Regal A', unitId: 'u1', slotIndex: 0, remainingWeightG: 700 });
  const AMS2 = spool({ id: 'am2', material: 'PLA', manufacturer: 'Polymaker', color: 'Schwarz', location: null, homeLocation: 'Regal A', unitId: 'u1', slotIndex: 2, remainingWeightG: 650 });

  function mockStock(list: FilamentSpool[]) {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') return Promise.resolve(list);
      if (cmd === 'list_printers') return Promise.resolve([X1C]);
      return Promise.resolve(undefined);
    });
  }
  const stat = (label: string) => screen.getByText(label).nextElementSibling;

  it('groups equal spools by default and counts single spools, kinds and grams in the header', async () => {
    mockStock([G1, G2, LOW, FULL, AMS1, AMS2]);
    renderView();
    const card = await screen.findByTestId('group-card-g1');
    expect(card).toHaveTextContent('2 Spulen');
    expect(card).toHaveTextContent('1.560,0 g');
    expect(screen.getByTestId('spool-card-low')).toBeInTheDocument();
    expect(stat('Spulen gesamt')).toHaveTextContent('6');
    // Shelf group, PETG, TPU and the group in the AMS.
    expect(stat('Verschiedene Sorten')).toHaveTextContent('4');
    expect(stat('Restbestand gesamt')).toHaveTextContent('4.030 g');
  });

  it('shows spools in a printer as a group of their own with the unit name', async () => {
    mockStock([G1, G2, AMS1, AMS2]);
    renderView();
    const card = await screen.findByTestId('group-card-am1');
    expect(card).toHaveTextContent('X1C · AMS A');
    expect(card).toHaveTextContent('Fach 1, Fach 3');
    expect(card).toHaveTextContent('Drucke buchen hier ab');
    expect(card).toHaveTextContent('1.350,0 g');
    expect(screen.getByTestId('group-card-g1')).not.toHaveTextContent('Drucke buchen hier ab');
  });

  it('orders the cards by their remaining share, lowest first', async () => {
    mockStock([FULL, G1, G2, LOW]);
    renderView();
    await screen.findByTestId('group-card-g1');
    const ids = screen.getAllByTestId(/^(group|spool)-card-/).map((el) => el.getAttribute('data-testid'));
    expect(ids).toEqual(['spool-card-low', 'group-card-g1', 'spool-card-full']);
  });

  it('switches to the single view, lists every shelf spool again and remembers the choice', async () => {
    mockStock([G1, G2, AMS1, AMS2]);
    const first = renderView();
    await screen.findByTestId('group-card-g1');
    fireEvent.click(screen.getByRole('button', { name: 'Einzeln' }));
    expect(screen.getByTestId('spool-card-g1')).toBeInTheDocument();
    expect(screen.getByTestId('spool-card-g2')).toBeInTheDocument();
    expect(screen.queryByTestId('group-card-g1')).toBeNull();
    // As before, loaded spools are only in the printer column here.
    expect(within(screen.getByTestId('filament-storage')).queryByTestId('spool-card-am1')).toBeNull();
    // The header keeps counting single spools, kinds are still groups.
    expect(stat('Spulen gesamt')).toHaveTextContent('4');
    expect(stat('Verschiedene Sorten')).toHaveTextContent('2');
    first.unmount();
    renderView();
    expect(await screen.findByTestId('spool-card-g1')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Einzeln' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'Gruppiert' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('applies the choice to the list as well', async () => {
    mockStock([G1, G2]);
    renderView();
    await screen.findByTestId('group-card-g1');
    fireEvent.click(screen.getByRole('button', { name: 'Liste' }));
    expect(screen.getByTestId('group-row-g1')).toBeInTheDocument();
    expect(screen.queryByTestId('spool-row-g1')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Einzeln' }));
    expect(screen.getByTestId('spool-row-g1')).toBeInTheDocument();
    expect(screen.getByTestId('spool-row-g2')).toBeInTheDocument();
    expect(screen.queryByTestId('group-row-g1')).toBeNull();
  });

  it('expands a group card to its spools', async () => {
    mockStock([G1, G2]);
    renderView();
    const toggle = await screen.findByRole('button', { name: 'PLA Schwarz: Spulen anzeigen' });
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByTestId('spool-row-g1')).toHaveTextContent('920,0 g');
    expect(screen.getByTestId('spool-row-g2')).toHaveTextContent('640,0 g');
  });

  it('filters "Niedrig" by the total of the group, not by single spools', async () => {
    // One nearly empty spool in a group that is full as a whole stays out of "Niedrig".
    const NEAR_EMPTY = spool({ id: 'ne', material: 'PLA', manufacturer: 'Polymaker', color: 'Schwarz', location: 'Regal A', remainingWeightG: 50 });
    mockStock([G1, NEAR_EMPTY, LOW]);
    renderView();
    await screen.findByTestId('group-card-g1');
    fireEvent.click(screen.getByRole('button', { name: 'Niedrig' }));
    expect(screen.getByTestId('spool-card-low')).toBeInTheDocument();
    expect(screen.queryByTestId('group-card-g1')).toBeNull();
  });

  it('counts groups as "Niedrig / leer" in the grouped view and spools in the single view', async () => {
    const A = spool({ id: 'x1', material: 'ABS', manufacturer: 'M', color: 'Grau', location: 'R', remainingWeightG: 100 });
    const B = spool({ id: 'x2', material: 'ABS', manufacturer: 'M', color: 'Grau', location: 'R', remainingWeightG: 100 });
    mockStock([A, B]);
    renderView();
    await screen.findByTestId('group-card-x1');
    expect(stat('Niedrig / leer')).toHaveTextContent('1');
    fireEvent.click(screen.getByRole('button', { name: 'Einzeln' }));
    expect(stat('Niedrig / leer')).toHaveTextContent('2');
  });

  it('finds a group by its manufacturer in the search', async () => {
    mockStock([G1, G2, LOW]);
    renderView();
    await screen.findByTestId('group-card-g1');
    fireEvent.change(screen.getByPlaceholderText(/durchsuchen/), { target: { value: 'anycubic' } });
    expect(screen.queryByTestId('group-card-g1')).toBeNull();
    expect(screen.getByTestId('spool-card-low')).toBeInTheDocument();
  });

  it('offers restock on the group card with the first spool as template', async () => {
    mockStock([G1, G2]);
    renderView();
    const card = await screen.findByTestId('group-card-g1');
    fireEvent.click(within(card).getByRole('button', { name: 'Nachkaufen' }));
    expect(await screen.findByRole('dialog', { name: /Nachkaufen/ })).toBeInTheDocument();
  });

  it('loads the spool with the smallest rest when a group card is dropped on a slot', async () => {
    mockStock([G1, G2, STORED]);
    renderView();
    const card = await screen.findByTestId('group-card-g1');
    fireEvent.mouseDown(card, { clientX: 0, clientY: 0, button: 0 });
    fireEvent.mouseMove(document, { clientX: 40, clientY: 40 });
    fireEvent.mouseEnter(screen.getByTestId('slot-u1-1'));
    fireEvent.mouseUp(document);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('load_spool', { spoolId: 'g2', unitId: 'u1', slotIndex: 1 }),
    );
  });
});
