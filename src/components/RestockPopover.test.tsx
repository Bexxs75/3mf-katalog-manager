import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { RestockPopover } from './RestockPopover';
import type { FilamentSpool } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const TEMPLATE: FilamentSpool = {
  id: 's2', material: 'ABS-T', manufacturer: 'Prusament', color: 'Orange', location: null,
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 120, price: 29.95, imagePng: null,
  colorHex: '#f07f1e', homeLocation: 'Technik', unitId: 'u1', slotIndex: 0, kind: 'filament',
};

let anchor: HTMLButtonElement;
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  localStorage.setItem('3mf-katalog-language', 'de');
  anchor = document.createElement('button');
  document.body.appendChild(anchor);
});
afterEach(() => anchor.remove());

function renderPopover(spool: FilamentSpool = TEMPLATE) {
  const onClose = vi.fn();
  const onCreated = vi.fn();
  render(
    <LanguageProvider>
      <RestockPopover spool={spool} anchor={anchor} knownLocations={['Technik', 'Regal 1']} onClose={onClose} onCreated={onCreated} />
    </LanguageProvider>,
  );
  return { onClose, onCreated };
}

describe('RestockPopover', () => {
  it('prefills amount, price and the home location of a loaded template', () => {
    renderPopover();
    expect(screen.getByRole('dialog', { name: 'Nachkaufen: ABS-T · Orange' })).toBeInTheDocument();
    expect(screen.getByLabelText('Gewicht (g)')).toHaveValue('1000');
    expect(screen.getByLabelText('Preis je Spule')).toHaveValue('29,95');
    expect(screen.getByDisplayValue('Technik')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '1 Spule anlegen' })).toBeInTheDocument();
  });

  it('uses bottle wording and ml for resin', () => {
    renderPopover({ ...TEMPLATE, kind: 'resin', originalWeightG: 500, unitId: null, slotIndex: null, location: 'Resin-Schrank' });
    expect(screen.getByText('Legt neue volle Flaschen mit denselben Daten an.')).toBeInTheDocument();
    expect(screen.getByLabelText('Inhalt (ml)')).toHaveValue('500');
    expect(screen.getByLabelText('Preis je Flasche')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '1 Flasche anlegen' })).toBeInTheDocument();
  });

  it('counts between 1 and 20 with its own buttons', () => {
    renderPopover();
    expect(screen.getByRole('button', { name: 'weniger' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'mehr' }));
    fireEvent.click(screen.getByRole('button', { name: 'mehr' }));
    expect(screen.getByRole('button', { name: '3 Spulen anlegen' })).toBeInTheDocument();
    for (let i = 0; i < 30; i++) fireEvent.click(screen.getByRole('button', { name: 'mehr' }));
    expect(screen.getByRole('button', { name: '20 Spulen anlegen' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'mehr' })).toBeDisabled();
  });

  it('creates the spools in one backend call and reports them', async () => {
    const created = [{ ...TEMPLATE, id: 'n1' }, { ...TEMPLATE, id: 'n2' }];
    vi.mocked(invoke).mockResolvedValue(created);
    const { onCreated } = renderPopover();
    fireEvent.click(screen.getByRole('button', { name: 'mehr' }));
    fireEvent.change(screen.getByLabelText('Preis je Spule'), { target: { value: '24,5' } });
    fireEvent.change(screen.getByDisplayValue('Technik'), { target: { value: 'Regal 1' } });
    fireEvent.click(screen.getByRole('button', { name: '2 Spulen anlegen' }));
    await waitFor(() => expect(onCreated).toHaveBeenCalledWith(created));
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('restock_filament_spool', {
      templateId: 's2', count: 2, weight: 1000, price: 24.5, location: 'Regal 1',
    });
  });

  it('rejects an invalid amount without calling the backend', () => {
    renderPopover();
    fireEvent.change(screen.getByLabelText('Gewicht (g)'), { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: '1 Spule anlegen' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Bitte eine Menge größer als 0 eingeben.');
    expect(invoke).not.toHaveBeenCalled();
  });

  it('shows an error and creates nothing when the backend refuses', async () => {
    vi.mocked(invoke).mockRejectedValue('Vorlage nicht gefunden');
    const { onCreated } = renderPopover();
    fireEvent.click(screen.getByRole('button', { name: '1 Spule anlegen' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Nachkaufen fehlgeschlagen');
    expect(onCreated).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: '1 Spule anlegen' })).toBeEnabled();
  });

  it('closes on Escape and on Abbrechen without creating anything', () => {
    const { onClose } = renderPopover();
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
    expect(onClose).toHaveBeenCalledTimes(2);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('closes on a click outside', () => {
    const { onClose } = renderPopover();
    fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenCalled();
  });
});
