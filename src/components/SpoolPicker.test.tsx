import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SpoolPicker } from './SpoolPicker';
import type { FilamentSpool } from '../types';

const spool = (id: string, material: string, color: string): FilamentSpool => ({
  id, material, manufacturer: null, color, location: null, diameterMm: 1.75, originalWeightG: 1000,
  remainingWeightG: 612.4, price: null, imagePng: null, colorHex: '#8a8f94', homeLocation: null, unitId: null, slotIndex: null,
} as FilamentSpool);

describe('SpoolPicker', () => {
  it('opens a listbox and selects with the keyboard', () => {
    const onChange = vi.fn();
    render(<SpoolPicker spools={[spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')]} value="1" onChange={onChange} label="Spule" />);
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'ArrowDown' });
    fireEvent.keyDown(list, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('2');
  });

  it('does not bubble Escape to the surrounding dialog (only closes its own list)', () => {
    const onChange = vi.fn();
    const outerKeyDown = vi.fn();
    render(
      <div onKeyDown={outerKeyDown}>
        <SpoolPicker spools={[spool('1', 'PLA', 'Grau')]} value="1" onChange={onChange} label="Spule" />
      </div>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Escape' });
    expect(outerKeyDown).not.toHaveBeenCalled();
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('exposes the active option via aria-activedescendant and moves it with ArrowDown', () => {
    const onChange = vi.fn();
    render(<SpoolPicker spools={[spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')]} value="1" onChange={onChange} label="Spule" />);
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    const list = screen.getByRole('listbox');
    const option1 = screen.getByRole('option', { name: /PLA/ });
    expect(list).toHaveAttribute('aria-activedescendant', option1.id);
    fireEvent.keyDown(list, { key: 'ArrowDown' });
    const option2 = screen.getByRole('option', { name: /PETG/ });
    expect(list).toHaveAttribute('aria-activedescendant', option2.id);
  });

  it('keeps the active option when the spool list re-renders with the same items while open', () => {
    const onChange = vi.fn();
    const first = [spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')];
    const { rerender } = render(<SpoolPicker spools={first} value="1" onChange={onChange} label="Spule" />);
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'ArrowDown' });
    const activeBefore = list.getAttribute('aria-activedescendant');
    // Neues Array mit denselben Eintraegen (z.B. nach einem previewJob-Refresh
    // aus der umgebenden PrinterJobsDialog) - die Tastatur-Position darf
    // dabei NICHT zurueckspringen.
    const second = [spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')];
    rerender(<SpoolPicker spools={second} value="1" onChange={onChange} label="Spule" />);
    expect(screen.getByRole('listbox')).toHaveAttribute('aria-activedescendant', activeBefore);
  });

  it('moves the active option only once it actually disappears from a refreshed list', () => {
    const onChange = vi.fn();
    const first = [spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')];
    const { rerender } = render(<SpoolPicker spools={first} value="1" onChange={onChange} label="Spule" />);
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'ArrowDown' }); // aktiv ist jetzt Spule 2 (PETG)
    rerender(<SpoolPicker spools={[spool('1', 'PLA', 'Grau')]} value="1" onChange={onChange} label="Spule" />);
    const remaining = screen.getByRole('option', { name: /PLA/ });
    expect(screen.getByRole('listbox')).toHaveAttribute('aria-activedescendant', remaining.id);
  });
});
