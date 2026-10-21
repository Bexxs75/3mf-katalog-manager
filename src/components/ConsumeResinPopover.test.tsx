import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ConsumeResinPopover } from './ConsumeResinPopover';
import type { FilamentSpool } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

const BOTTLE: FilamentSpool = {
  id: 'r1', material: 'Standard', manufacturer: 'Elegoo', color: 'Grau', location: 'Resin-Schrank',
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 640, price: 27.99, imagePng: null,
  colorHex: '#8a8f98', colorHexes: [], homeLocation: null, unitId: null, slotIndex: null, kind: 'resin',
};

let anchor: HTMLButtonElement;
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  localStorage.setItem('3mf-katalog-language', 'de');
  anchor = document.createElement('button');
  document.body.appendChild(anchor);
});
afterEach(() => anchor.remove());

function renderPopover() {
  const onClose = vi.fn();
  const onConsumed = vi.fn();
  render(
    <LanguageProvider>
      <ConsumeResinPopover spool={BOTTLE} anchor={anchor} onClose={onClose} onConsumed={onConsumed} />
    </LanguageProvider>,
  );
  return { onClose, onConsumed };
}

describe('ConsumeResinPopover', () => {
  it('shows what is left and deducts the entered amount', async () => {
    const updated = { ...BOTTLE, remainingWeightG: 594.5 };
    vi.mocked(invoke).mockResolvedValue(updated);
    const { onConsumed } = renderPopover();
    expect(screen.getByRole('dialog', { name: 'Verbrauch: Standard · Grau' })).toBeInTheDocument();
    expect(screen.getByText('Noch 640 ml in der Flasche.')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Verbraucht (ml)'), { target: { value: '45,5' } });
    fireEvent.click(screen.getByRole('button', { name: 'Abbuchen' }));
    await waitFor(() => expect(onConsumed).toHaveBeenCalledWith(updated));
    expect(invoke).toHaveBeenCalledWith('consume_resin', { spoolId: 'r1', amountMl: 45.5 });
  });

  it('rejects an empty or zero amount without calling the backend', () => {
    renderPopover();
    fireEvent.click(screen.getByRole('button', { name: 'Abbuchen' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Bitte eine Menge größer als 0 eingeben.');
    expect(invoke).not.toHaveBeenCalled();
  });

  it('reports a backend error and keeps the stock', async () => {
    vi.mocked(invoke).mockRejectedValue('Flasche nicht gefunden');
    const { onConsumed } = renderPopover();
    fireEvent.change(screen.getByLabelText('Verbraucht (ml)'), { target: { value: '10' } });
    fireEvent.click(screen.getByRole('button', { name: 'Abbuchen' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Abbuchen fehlgeschlagen');
    expect(onConsumed).not.toHaveBeenCalled();
  });

  it('closes on Escape without deducting', () => {
    const { onClose } = renderPopover();
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe('ConsumeResinPopover with several bottles', () => {
  const bottle = (id: string, remainingWeightG: number): FilamentSpool => ({ ...BOTTLE, id, remainingWeightG });
  const choices = [bottle('r1', 800), bottle('r2', 200), bottle('r3', 0), bottle('r4', 500)];

  function renderChoices() {
    const onConsumed = vi.fn();
    render(
      <LanguageProvider>
        <ConsumeResinPopover spool={choices[1]} choices={choices} anchor={anchor} onClose={vi.fn()} onConsumed={onConsumed} />
      </LanguageProvider>,
    );
    return onConsumed;
  }
  const picker = () => screen.getByLabelText('Flasche') as HTMLSelectElement;

  it('preselects the bottle with the smallest rest that is not empty', () => {
    renderChoices();
    expect(picker().value).toBe('r2');
    expect(screen.getByRole('option', { name: /Vorschlag/ })).toHaveValue('r2');
  });

  it('moves the preselection to the smallest bottle that still covers the amount', () => {
    renderChoices();
    fireEvent.change(screen.getByLabelText('Verbraucht (ml)'), { target: { value: '300' } });
    expect(picker().value).toBe('r4');
    fireEvent.change(screen.getByLabelText('Verbraucht (ml)'), { target: { value: '700' } });
    expect(picker().value).toBe('r1');
  });

  it('keeps a bottle the user picked, whatever amount is typed afterwards', () => {
    renderChoices();
    fireEvent.change(picker(), { target: { value: 'r1' } });
    fireEvent.change(screen.getByLabelText('Verbraucht (ml)'), { target: { value: '50' } });
    expect(picker().value).toBe('r1');
  });

  it('deducts from the selected bottle', async () => {
    vi.mocked(invoke).mockResolvedValue({ ...choices[3], remainingWeightG: 200 });
    const onConsumed = renderChoices();
    fireEvent.change(screen.getByLabelText('Verbraucht (ml)'), { target: { value: '300' } });
    fireEvent.click(screen.getByRole('button', { name: 'Abbuchen' }));
    await waitFor(() => expect(onConsumed).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith('consume_resin', { spoolId: 'r4', amountMl: 300 });
  });

  it('has no bottle picker for a single bottle', () => {
    renderPopover();
    expect(screen.queryByLabelText('Flasche')).toBeNull();
  });
});
