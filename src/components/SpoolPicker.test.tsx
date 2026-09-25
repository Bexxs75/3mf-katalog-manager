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
});
