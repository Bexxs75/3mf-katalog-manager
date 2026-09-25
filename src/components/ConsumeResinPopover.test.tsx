import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ConsumeResinPopover } from './ConsumeResinPopover';
import type { FilamentSpool } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const BOTTLE: FilamentSpool = {
  id: 'r1', material: 'Standard', manufacturer: 'Elegoo', color: 'Grau', location: 'Resin-Schrank',
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 640, price: 27.99, imagePng: null,
  colorHex: '#8a8f98', homeLocation: null, unitId: null, slotIndex: null, kind: 'resin',
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
