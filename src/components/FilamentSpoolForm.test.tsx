import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { FilamentSpoolForm } from './FilamentSpoolForm';
import type { FilamentSpool } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); vi.mocked(invoke).mockResolvedValue(undefined); });

const LOADED: FilamentSpool = {
  id: 's1', material: 'PLA', manufacturer: 'Bambu Lab', color: 'Galaxy Black', location: null,
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 600, price: null, imagePng: null,
  colorHex: '#1a1a1a', homeLocation: 'Regal 2', unitId: 'u1', slotIndex: 0,
};

function renderForm(editing: FilamentSpool | null) {
  localStorage.setItem('3mf-katalog-language', 'de');
  const onSaved = vi.fn();
  render(
    <LanguageProvider>
      <FilamentSpoolForm open editing={editing} knownLocations={[]} onClose={vi.fn()} onSaved={onSaved} />
    </LanguageProvider>,
  );
  return onSaved;
}

describe('FilamentSpoolForm', () => {
  it('edits the home location of a spool that sits in a slot', () => {
    renderForm(LOADED);
    expect(screen.getByText('Stammplatz')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Regal 2')).toBeInTheDocument();
  });

  it('sends the chosen color value with the spool', async () => {
    const onSaved = renderForm(LOADED);
    fireEvent.click(screen.getByRole('radio', { name: '#27ae60' }));
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith(
      'update_filament_spool',
      expect.objectContaining({
        spool: expect.objectContaining({ colorHex: '#27ae60', location: 'Regal 2', unitId: 'u1', slotIndex: 0 }),
      }),
    );
  });

  it('shows the normal storage label for a spool in storage', () => {
    renderForm({ ...LOADED, unitId: null, slotIndex: null, homeLocation: null, location: 'Regal 1' });
    expect(screen.getByText('Lagerort')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Regal 1')).toBeInTheDocument();
  });
});
