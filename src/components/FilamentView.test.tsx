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
    colorHex: '#1a1a1a', homeLocation: null, unitId: null, slotIndex: null, kind: 'filament', ...overrides,
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
  units: [{ id: 'u1', printerId: 'p1', name: 'AMS A', kind: 'bambu_ams', slotCount: 4, bambuAmsIndex: 0 }],
};

const SATURN: Printer = {
  id: 'p9',
  name: 'Saturn 4',
  kind: 'resin',
  units: [{ id: 'vat', printerId: 'p9', name: 'Harzwanne', kind: 'resin_vat', slotCount: 1, bambuAmsIndex: null }],
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

// Spiegelt App.tsx: eine einzige `usePrinters()`-Instanz, die an FilamentView
// (und in der echten App zugleich an Rail) weitergereicht wird.
function PrintersWrapper({ children }: { children: (printers: ReturnType<typeof usePrinters>) => ReactNode }) {
  const printers = usePrinters();
  return <>{children(printers)}</>;
}

function renderView() {
  render(
    <LanguageProvider>
      <PrintersWrapper>{(printers) => <FilamentView printerLink={printerLink()} printers={printers} />}</PrintersWrapper>
    </LanguageProvider>,
  );
}

describe('FilamentView with printers', () => {
  it('shows loaded spools only in the printer column but counts all spools', async () => {
    renderView();
    await waitFor(() => expect(screen.getByTestId('slot-u1-0')).toHaveTextContent('PLA · Schwarz'));
    const storage = screen.getByTestId('filament-storage');
    expect(within(storage).queryByTestId('spool-card-in')).toBeNull();
    expect(within(storage).getByTestId('spool-card-store')).toBeInTheDocument();
    expect(screen.getByText('Spulen gesamt').nextElementSibling).toHaveTextContent('2');
  });

  it('counts a loaded spool\'s home location in the storage-location stat', async () => {
    // LOADED hat `location: null` (steckt im Fach) aber `homeLocation:
    // 'Regal 2'`; STORED liegt unter `location: 'Regal 1'` im Lager. Beide
    // Orte muessen gezaehlt werden, nicht nur der von STORED.
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
    expect(await screen.findByTestId('spool-card-new1')).toHaveClass('spool-new');
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
    // Die Druckerspalte zeigt in der Resin-Ansicht nur Resin-Drucker (v0.14.0).
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
    // Filament-Ansicht: ein Resin-Drucker ist hier gar nicht sichtbar; ein
    // Ziehen auf eine Stelle ohne passendes Ziel loest nichts aus.
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
    // 620,3 g + 620,4 g = 1240,7 g - die Summe der Zehntelgramm-genauen
    // Restgewichte darf im Statistik-Kachel nicht mit Nachkommastelle
    // erscheinen (Spec: ganze Gramm).
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
    // Regression fuer Task-12-Review-Fund: App.tsx darf `usePrinters()` nur
    // einmal aufrufen und dieselbe Instanz an FilamentView UND an Rail
    // weiterreichen - sonst sieht der Rail-Reiter Aenderungen aus dem
    // Filament-Lager erst nach einem Remount.
    let printerAdded = false;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_filament_spools') return Promise.resolve([LOADED, STORED]);
      if (cmd === 'list_printers') {
        return Promise.resolve(printerAdded ? [X1C, { id: 'p2', name: 'Neuer Drucker', kind: 'filament', units: [] }] : [X1C]);
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
              {/* Steht hier fuer einen zweiten Verbraucher derselben Instanz, z.B. Rail. */}
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
    // Derselbe Refresh muss auch in FilamentViews eigener Druckerspalte ankommen.
    expect(printerColumn()).toHaveTextContent('Neuer Drucker');
  });
});

describe('FilamentView printer-jobs dialog state', () => {
  it('resets jobsOpen once the job list empties, so the banner returns and no dialog pops up unasked', async () => {
    const { rerender } = render(<JobsWrapper jobs={[printerJob({ id: '1' })]} />);
    await waitFor(() => screen.getByTestId('slot-u1-0'));

    fireEvent.click(screen.getByRole('button', { name: 'Prüfen' }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Liste leert sich (z.B. nach dem letzten Bestaetigen/Ignorieren) -
    // ohne Reset bliebe der Banner dauerhaft ausgeblendet.
    rerender(<JobsWrapper jobs={[]} />);
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

    // Neue Drucke kommen herein - der Dialog darf NICHT ungefragt wieder
    // aufspringen, der Banner muss stattdessen zurueck sein.
    rerender(<JobsWrapper jobs={[printerJob({ id: '2' })]} />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Prüfen' })).toBeInTheDocument();
  });
});
