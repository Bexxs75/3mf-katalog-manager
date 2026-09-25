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

function spool(overrides: Partial<FilamentSpool>): FilamentSpool {
  return {
    id: 's1', material: 'PLA', manufacturer: null, color: 'Schwarz', location: null,
    diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 620, price: null, imagePng: null,
    colorHex: '#1a1a1a', homeLocation: null, unitId: null, slotIndex: null, ...overrides,
  };
}

const LOADED = spool({ id: 'in', homeLocation: 'Regal 2', unitId: 'u1', slotIndex: 0 });
const STORED = spool({ id: 'store', material: 'PETG', color: 'Rot', location: 'Regal 1', colorHex: '#c0392b' });
const X1C: Printer = {
  id: 'p1',
  name: 'X1C',
  units: [{ id: 'u1', printerId: 'p1', name: 'AMS A', kind: 'bambu_ams', slotCount: 4, bambuAmsIndex: 0 }],
};

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === 'list_filament_spools') return Promise.resolve([LOADED, STORED]);
    if (cmd === 'list_printers') return Promise.resolve([X1C]);
    if (cmd === 'unload_spool') return Promise.resolve('Regal 2');
    if (cmd === 'load_spool') return Promise.resolve({ displacedSpoolId: null });
    if (cmd === 'list_file_summaries') return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
  localStorage.setItem('3mf-katalog-language', 'de');
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
        return Promise.resolve(printerAdded ? [X1C, { id: 'p2', name: 'Neuer Drucker', units: [] }] : [X1C]);
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
              <button onClick={() => printers.addPrinter('Neuer Drucker', 'Halter')}>Drucker hinzufuegen</button>
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
