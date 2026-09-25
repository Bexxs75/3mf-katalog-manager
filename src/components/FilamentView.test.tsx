import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { FilamentView } from './FilamentView';
import type { FilamentSpool, Printer } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

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
  units: [{ id: 'u1', printerId: 'p1', name: 'AMS A', kind: 'bambu_ams', slotCount: 4, bambuAmsIndex: 0 }],
};

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === 'list_filament_spools') return Promise.resolve([LOADED, STORED, RESIN]);
    if (cmd === 'list_printers') return Promise.resolve([X1C]);
    if (cmd === 'unload_spool') return Promise.resolve('Regal 2');
    if (cmd === 'load_spool') return Promise.resolve({ displacedSpoolId: null });
    return Promise.resolve(undefined);
  });
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.removeItem('3mf-katalog-filament-kind');
});

function renderView() {
  render(
    <LanguageProvider>
      <FilamentView />
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
    // Die Druckerspalte bleibt und zeigt weiter das Filament im Fach.
    expect(screen.getByTestId('slot-u1-0')).toHaveTextContent('PLA');
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
        <FilamentView />
      </LanguageProvider>,
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Resin' }));
    first.unmount();
    renderView();
    expect(await screen.findByTestId('spool-card-resin1')).toBeInTheDocument();
  });
});
